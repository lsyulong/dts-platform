use super::{
    MaskingPlan, MaskingReceipt, MaskingService, bad, denied,
    policy::parameters,
    types::{BoundRule, OutputMask},
};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::{AuthorizedQuery, masking_outputs, masking_table},
        query_svc::QueryResult,
    },
};
use ring::digest::{SHA256, digest};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl MaskingService {
    pub async fn prepare_output(
        &self,
        actor: &AuthIdentity,
        query: &AuthorizedQuery,
    ) -> Result<MaskingPlan, AppError> {
        let before = self.policy_versions(&actor.tenant_id).await?;
        let mut plan = self.prepare_output_inner(actor, query).await?;
        if self.policy_versions(&actor.tenant_id).await? != before {
            return Err(denied("输出策略在准备期间变化，请重试"));
        }
        plan.policy_versions = before;
        Ok(plan)
    }

    async fn policy_versions(
        &self,
        tenant: &str,
    ) -> Result<Vec<dts_backend::repo::data_access::PrivacyPolicyVersion>, AppError> {
        let mut versions = Vec::new();
        for kind in ["masking_rule", "user_group"] {
            for resource in self.published(tenant, kind).await? {
                versions.push(dts_backend::repo::data_access::PrivacyPolicyVersion {
                    kind: kind.into(),
                    id: resource.id,
                    version: resource.version,
                });
            }
        }
        versions.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
        Ok(versions)
    }

    async fn prepare_output_inner(
        &self,
        actor: &AuthIdentity,
        query: &AuthorizedQuery,
    ) -> Result<MaskingPlan, AppError> {
        let (tenant, owner) = query.actor_scope();
        if tenant != actor.tenant_id || owner != actor.sub {
            return Err(denied("脱敏计划不能转交其他用户或租户"));
        }
        // Only the opaque, database-verified quality compiler token reaches this
        // branch. Its fixed output contains aggregate counters, never row samples.
        if query.is_controlled_quality_count() {
            self.active_user(&actor.tenant_id, &actor.sub).await?;
            return Ok(MaskingPlan {
                query: query.clone(),
                policy_hash: hash(&json!({"version":1,"output":"quality_count_v1"}))?,
                fields: vec![],
                policy_versions: vec![],
            });
        }
        let rules = self.applicable(actor).await?;
        if rules.is_empty() {
            return empty_plan(query);
        }
        // Complex admin SQL must fail closed whenever this user has a policy;
        // tableless constants work when there is no active applicable policy.
        let table = masking_table(query.sql())?;
        let rules: Vec<_> = rules
            .into_iter()
            .filter(|rule| {
                table
                    .iter()
                    .zip([
                        &rule.target.binding.table_catalog,
                        &rule.target.binding.schema_name,
                        &rule.target.binding.table_name,
                    ])
                    .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
            })
            .collect();
        if rules.is_empty() {
            return empty_plan(query);
        }
        let mut by_column = BTreeMap::new();
        for rule in &rules {
            if by_column
                .insert(rule.definition.column_name.clone(), rule)
                .is_some()
            {
                return Err(denied("同一字段有多个适用脱敏规则，请解决策略冲突"));
            }
        }
        let sensitive = by_column.keys().cloned().collect();
        let projections = masking_outputs(query.sql(), &rules[0].target, &sensitive)?;
        let fields = projections
            .into_iter()
            .map(|(output, source)| OutputMask {
                output_name: output,
                rule: (*by_column[&source]).clone(),
            })
            .collect();
        Ok(MaskingPlan {
            query: query.clone(),
            policy_hash: hash(&json!({"version":1,"rules":rules}))?,
            fields,
            policy_versions: vec![],
        })
    }

    pub async fn reauthorize_plan(
        &self,
        actor: &AuthIdentity,
        plan: &MaskingPlan,
    ) -> Result<(), AppError> {
        let latest = self.prepare_output(actor, &plan.query).await?;
        if latest.policy_hash != plan.policy_hash || latest.policy_versions != plan.policy_versions
        {
            return Err(denied("查询期间适用脱敏规则或用户组变化，不允许释放结果"));
        }
        Ok(())
    }

    pub fn apply_output(
        &self,
        plan: &MaskingPlan,
        mut result: QueryResult,
    ) -> Result<QueryResult, AppError> {
        result_shape(&result)?;
        let mut resolved = Vec::new();
        for field in &plan.fields {
            let column = result
                .columns
                .iter_mut()
                .find(|column| column.name.eq_ignore_ascii_case(&field.output_name))
                .ok_or_else(|| denied("查询输出缺少已绑定的脱敏字段"))?;
            column.data_type = "VARCHAR".into();
            column.is_numeric = false;
            resolved.push((column.name.clone(), &field.rule));
        }
        for row in &mut result.rows {
            let map = row
                .as_object_mut()
                .ok_or_else(|| bad("查询结果行必须是对象"))?;
            for (name, rule) in &resolved {
                let value = map
                    .get(name)
                    .ok_or_else(|| denied("查询结果行缺少脱敏字段"))?;
                let masked = self.mask_value(plan.query.actor_scope().0, rule, value)?;
                map.insert(name.clone(), masked);
            }
        }
        if plan.has_masks() {
            result.suggested_chart = None;
        }
        if serde_json::to_vec(&result)
            .map_err(|_| bad("脱敏结果无法编码"))?
            .len()
            > 8 * 1024 * 1024
        {
            return Err(bad("脱敏后查询结果超过8MiB"));
        }
        Ok(result)
    }

    pub fn seal_output(
        &self,
        plan: &MaskingPlan,
        result: &QueryResult,
    ) -> Result<MaskingReceipt, AppError> {
        result_shape(result)?;
        for field in &plan.fields {
            let column = result
                .columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(&field.output_name))
                .ok_or_else(|| denied("脱敏收据缺少输出字段"))?;
            if column.data_type != "VARCHAR" || column.is_numeric {
                return Err(denied("不能为未转换的脱敏结果生成收据"));
            }
        }
        let mut receipt = MaskingReceipt {
            version: 1,
            tenant_id: plan.query.actor_scope().0.into(),
            query_hash: plan.query.evidence().query_hash.clone(),
            policy_hash: plan.policy_hash.clone(),
            result_hash: hash(result)?,
            signature: String::new(),
        };
        receipt.signature = self.key.sign(&receipt_bytes(&receipt)?);
        Ok(receipt)
    }

    pub async fn apply_stored_output(
        &self,
        actor: &AuthIdentity,
        query: &AuthorizedQuery,
        result: QueryResult,
        receipt: Option<&MaskingReceipt>,
    ) -> Result<QueryResult, AppError> {
        let plan = self.prepare_output(actor, query).await?;
        let result = match receipt {
            None => self.apply_output(&plan, result)?,
            Some(receipt) => {
                result_shape(&result)?;
                if receipt.version != 1
                    || receipt.tenant_id != actor.tenant_id
                    || receipt.query_hash != query.evidence().query_hash
                    || receipt.result_hash != hash(&result)?
                    || !self
                        .key
                        .verify(&receipt_bytes(receipt)?, &receipt.signature)
                {
                    return Err(denied("历史脱敏结果收据不完整或签名无效"));
                }
                if receipt.policy_hash != plan.policy_hash {
                    return Err(denied("历史结果的脱敏策略已变化，原值不可恢复，请重新执行"));
                }
                result
            },
        };
        self.reauthorize_plan(actor, &plan).await?;
        Ok(result)
    }

    pub(super) fn mask_value(
        &self,
        tenant: &str,
        rule: &BoundRule,
        value: &Value,
    ) -> Result<Value, AppError> {
        if value.is_null() {
            return Ok(Value::Null);
        }
        let (start, end) = parameters(&rule.definition)?;
        match rule.definition.method.as_str() {
            "redact" => Ok(json!("***")),
            "hash" => {
                let input = serde_json::to_vec(&json!([
                    "dts.privacy.value.v1",
                    tenant,
                    rule.target.metadata_id,
                    rule.definition.column_name,
                    rule.definition.column_data_type,
                    value
                ]))
                .map_err(|_| bad("敏感值无法编码"))?;
                Ok(json!(format!("hmac:v1:{}", self.key.sign(&input))))
            },
            "partial" => {
                let text = value
                    .as_str()
                    .ok_or_else(|| denied("实际部分保留值不属于字符串"))?;
                let chars: Vec<_> = text.chars().collect();
                if chars.len() <= start + end {
                    return Ok(json!("***"));
                }
                let prefix: String = chars[..start].iter().collect();
                let suffix: String = chars[chars.len() - end..].iter().collect();
                Ok(json!(format!("{prefix}***{suffix}")))
            },
            _ => Err(bad("脱敏方式不受支持")),
        }
    }
}

fn empty_plan(query: &AuthorizedQuery) -> Result<MaskingPlan, AppError> {
    Ok(MaskingPlan {
        query: query.clone(),
        policy_hash: hash(&json!({"version":1,"rules":[]}))?,
        fields: vec![],
        policy_versions: vec![],
    })
}
pub(super) fn hash(value: &impl Serialize) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value).map_err(|_| bad("脱敏证据无法编码"))?;
    let digest = digest(&SHA256, &bytes);
    Ok(digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn receipt_bytes(receipt: &MaskingReceipt) -> Result<Vec<u8>, AppError> {
    if receipt.tenant_id.len() > 256
        || [
            &receipt.query_hash,
            &receipt.policy_hash,
            &receipt.result_hash,
        ]
        .iter()
        .any(|value| value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || receipt.signature.len() > 128
    {
        return Err(denied("脱敏收据字段无效"));
    }
    serde_json::to_vec(&json!([
        "dts.privacy.result.v1",
        receipt.version,
        receipt.tenant_id,
        receipt.query_hash,
        receipt.policy_hash,
        receipt.result_hash
    ]))
    .map_err(|_| bad("脱敏收据无法编码"))
}
fn result_shape(result: &QueryResult) -> Result<(), AppError> {
    if result.row_count != result.rows.len()
        || result.columns.len() > 1000
        || result.rows.len() > 10000
    {
        return Err(denied("查询结果行数或字段数量不一致"));
    }
    let mut names = BTreeSet::new();
    if result.columns.iter().any(|column| {
        column.name.is_empty()
            || column.name.len() > 256
            || !names.insert(column.name.to_ascii_lowercase())
    }) {
        return Err(denied("查询输出字段为空、重复或过长"));
    }
    let exact: BTreeSet<_> = result
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    if result.rows.iter().any(|row| {
        row.as_object().is_none_or(|map| {
            map.len() != exact.len() || map.keys().any(|key| !exact.contains(key.as_str()))
        })
    }) {
        return Err(denied("查询行字段与输出Schema不一致"));
    }
    Ok(())
}
