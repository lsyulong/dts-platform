use super::{
    MaskingService, bad, denied,
    types::{BoundRule, GroupDefinition, RuleDefinition},
};
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{
    ProductResource, ProductResourceFilter,
    repo::{
        data_access::DataAccessTarget,
        metadata_catalog::{MetadataColumn, binding_matches},
    },
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

impl MaskingService {
    pub(super) async fn active_user(&self, tenant: &str, id: &str) -> Result<(), AppError> {
        let detail = self
            .users
            .find_detail_by_id(id)
            .await?
            .filter(|user| user.tenant_id == tenant)
            .ok_or_else(|| denied("脱敏用户不存在或不属于当前租户"))?;
        let user = self
            .users
            .find_by_tenant_and_username(tenant, &detail.username)
            .await?
            .filter(|user| user.id == id && user.tenant_id == tenant && user.status == "active")
            .ok_or_else(|| denied("脱敏用户已停用或不存在"))?;
        let _ = user;
        Ok(())
    }

    pub(super) async fn published(
        &self,
        tenant: &str,
        kind: &str,
    ) -> Result<Vec<ProductResource>, AppError> {
        let mut data = Vec::new();
        loop {
            let page = self
                .resources
                .list(&ProductResourceFilter {
                    tenant_id: tenant.into(),
                    kind: kind.into(),
                    q: None,
                    status: Some("published".into()),
                    created_by: None,
                    limit: 100,
                    offset: data.len() as u32,
                })
                .await?;
            if page.total > 1000 || page.total < 0 {
                return Err(denied("脱敏策略数量超过安全扫描上限"));
            }
            if page.data.is_empty() {
                if data.len() as i64 != page.total {
                    return Err(denied("脱敏策略分页发生变化，请重试"));
                }
                break;
            }
            if page.data.iter().any(|record| {
                record.tenant_id != tenant || record.kind != kind || record.status != "published"
            }) {
                return Err(denied("脱敏策略仓储返回越域记录"));
            }
            data.extend(page.data);
            if data.len() > 1000 {
                return Err(denied("脱敏策略数量超过安全扫描上限"));
            }
            if data.len() as i64 == page.total {
                break;
            }
        }
        let mut seen = BTreeSet::new();
        if data.iter().any(|record| !seen.insert(record.id.clone())) {
            return Err(denied("脱敏策略分页包含重复记录"));
        }
        Ok(data)
    }

    pub(super) async fn bind_rule(
        &self,
        tenant: &str,
        rule: &RuleDefinition,
        frozen: bool,
    ) -> Result<DataAccessTarget, AppError> {
        identifier(&rule.object_id)?;
        identifier(&rule.column_name)?;
        parameters(rule)?;
        let metadata = self
            .resources
            .get(tenant, "metadata_object", &rule.object_id)
            .await?
            .filter(|resource| resource.tenant_id == tenant && resource.kind == "metadata_object")
            .ok_or_else(|| bad("脱敏规则引用的元数据对象不存在"))?;
        let binding = self
            .metadata
            .verified_binding(tenant, &metadata.id, metadata.version)
            .await?
            .filter(|binding| {
                binding.tenant_id == tenant
                    && binding.engine == "swanlake"
                    && binding.object_version == metadata.version
                    && binding_matches(binding, &metadata.properties)
            })
            .ok_or_else(|| denied("脱敏字段没有真实且当前有效的元数据采集证明"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(metadata.properties["columns"].clone())
                .map_err(|_| bad("真实元数据字段快照无效"))?;
        if columns.is_empty() || columns.len() > 1000 {
            return Err(bad("真实字段数量无效"));
        }
        let column = columns
            .iter()
            .find(|column| column.name == rule.column_name)
            .ok_or_else(|| bad("脱敏字段不在真实元数据对象中"))?;
        if rule.method == "partial" && column.data_type != "VARCHAR" {
            return Err(bad("部分保留只支持实际VARCHAR字段"));
        }
        if frozen
            && (rule.metadata_version != Some(metadata.version)
                || rule.column_data_type.as_deref() != Some(column.data_type.as_str())
                || rule.source_fingerprint.as_deref() != Some(binding.source_fingerprint.as_str()))
        {
            return Err(denied("脱敏规则发布时的字段绑定已变化，请重新校验发布"));
        }
        Ok(DataAccessTarget {
            metadata_id: metadata.id,
            binding,
            columns,
        })
    }

    pub(super) async fn applicable(
        &self,
        actor: &AuthIdentity,
    ) -> Result<Vec<BoundRule>, AppError> {
        self.active_user(&actor.tenant_id, &actor.sub).await?;
        let mut selected: BTreeMap<String, Vec<(String, i64)>> = BTreeMap::new();
        for group in self.published(&actor.tenant_id, "user_group").await? {
            let definition = group_definition(&group.properties)?;
            if !definition.member_ids.contains(&actor.sub) {
                continue;
            }
            let actual_members: BTreeSet<_> = group
                .references
                .iter()
                .filter(|reference| {
                    reference.field == "member_ids" && reference.target_kind == "user"
                })
                .map(|reference| reference.target_id.as_str())
                .collect();
            if actual_members != definition.member_ids.iter().map(String::as_str).collect() {
                return Err(denied("已发布脱敏用户组的真实成员引用不一致"));
            }
            if let Some(id) = definition.masking_rule_id {
                if !group.references.iter().any(|reference| {
                    reference.field == "masking_rule_id"
                        && reference.target_kind == "masking_rule"
                        && reference.target_id == id
                }) {
                    return Err(denied("已发布用户组的真实脱敏规则引用不一致"));
                }
                selected
                    .entry(id)
                    .or_default()
                    .push((group.id, group.version));
            }
        }
        let mut rules = Vec::new();
        let mut encoded_bytes = 0usize;
        for (id, mut groups) in selected {
            let resource = self
                .resources
                .get(&actor.tenant_id, "masking_rule", &id)
                .await?
                .filter(|resource| {
                    resource.tenant_id == actor.tenant_id && resource.kind == "masking_rule"
                })
                .ok_or_else(|| denied("已发布用户组引用的脱敏规则不存在"))?;
            // A withdrawn rule is deliberately inactive, never an implicit policy.
            if resource.status != "published" {
                continue;
            }
            let definition = rule_definition(&resource.properties)?;
            if !resource.references.iter().any(|reference| {
                reference.field == "object_id"
                    && reference.target_kind == "metadata_object"
                    && reference.target_id == definition.object_id
            }) {
                return Err(denied("已发布脱敏规则的真实对象引用不一致"));
            }
            let target = self.bind_rule(&actor.tenant_id, &definition, true).await?;
            groups.sort();
            let rule = BoundRule {
                rule_id: id,
                rule_version: resource.version,
                groups,
                definition,
                target,
            };
            encoded_bytes = encoded_bytes
                .checked_add(
                    serde_json::to_vec(&rule)
                        .map_err(|_| bad("脱敏策略无法编码"))?
                        .len(),
                )
                .ok_or_else(|| denied("脱敏策略过大"))?;
            if encoded_bytes > 8 * 1024 * 1024 {
                return Err(denied("脱敏策略超过8MiB安全上限"));
            }
            rules.push(rule);
        }
        Ok(rules)
    }
}

pub(super) fn rule_definition(value: &Value) -> Result<RuleDefinition, AppError> {
    serde_json::from_value(value.clone()).map_err(|_| bad("脱敏规则字段或类型无效"))
}
pub(super) fn group_definition(value: &Value) -> Result<GroupDefinition, AppError> {
    let group: GroupDefinition =
        serde_json::from_value(value.clone()).map_err(|_| bad("脱敏用户组字段或类型无效"))?;
    if group.member_ids.is_empty() || group.member_ids.len() > 500 {
        return Err(bad("脱敏用户组须包含1至500个真实成员"));
    }
    let mut seen = BTreeSet::new();
    for id in &group.member_ids {
        identifier(id)?;
        if !seen.insert(id) {
            return Err(bad("脱敏用户组成员重复"));
        }
    }
    if let Some(id) = &group.masking_rule_id {
        identifier(id)?;
    }
    Ok(group)
}

pub(super) fn identifier(value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(bad("脱敏引用标识为空、过长或包含控制字符"))
    } else {
        Ok(())
    }
}

pub(super) fn parameters(rule: &RuleDefinition) -> Result<(usize, usize), AppError> {
    if !matches!(rule.method.as_str(), "redact" | "hash" | "partial") {
        return Err(bad("脱敏方式不受支持"));
    }
    if rule.parameters.is_null() {
        return Ok((2, 2));
    }
    let map = rule
        .parameters
        .as_object()
        .ok_or_else(|| bad("脱敏参数必须是对象"))?;
    if rule.method != "partial" {
        if !map.is_empty() {
            return Err(bad("遮盖和摘要不接受自定义参数或普通JSON密钥"));
        }
        return Ok((0, 0));
    }
    if map
        .keys()
        .any(|key| !matches!(key.as_str(), "keep_start" | "keep_end"))
    {
        return Err(bad("部分保留参数仅支持keep_start/keep_end"));
    }
    let number = |field: &str| -> Result<usize, AppError> {
        match map.get(field) {
            None => Ok(2),
            Some(value) => value
                .as_u64()
                .filter(|value| *value <= 32)
                .map(|value| value as usize)
                .ok_or_else(|| bad("部分保留字符数须为0至32的整数")),
        }
    };
    Ok((number("keep_start")?, number("keep_end")?))
}
