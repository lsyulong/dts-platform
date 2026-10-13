use super::{StandardGovernanceService, validation};
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{ProductResource, repo::standard_governance::*};
use serde_json::Value;

// Constructed only after the transactional repository has claimed a real model.
pub struct ModelStandardPublishClaim {
    record: StandardModelPublicationClaimRecord,
}
impl ModelStandardPublishClaim {
    pub fn resource(&self) -> &ProductResource {
        &self.record.resource
    }
    pub fn standards_state(&self) -> ModelStandardsState {
        self.record.standards_state
    }
    pub fn gate_id(&self) -> &str {
        &self.record.gate_id
    }
    pub fn needs_reconciliation(&self) -> bool {
        self.record.needs_reconciliation
    }
}
impl StandardGovernanceService {
    pub async fn reauthorize_model_publication(
        &self,
        actor: &AuthIdentity,
        claim: &ModelStandardPublishClaim,
    ) -> Result<(), AppError> {
        if actor.tenant_id != claim.record.tenant_id || actor.sub != claim.record.actor_id {
            return Err(AppError::Forbidden("模型发布占用不属于当前身份".into()));
        }
        self.authorize(
            actor,
            &[
                "models:read",
                "models:write",
                "models:execute",
                "standards:read",
            ],
        )
        .await?;
        let current = self
            .resource(
                actor,
                "data_model",
                &claim.record.resource.id,
                claim.record.claim_version,
            )
            .await?;
        if current.status != "publishing" || current.properties != claim.record.resource.properties
        {
            return Err(AppError::Conflict(
                "模型发布占用已被替换或定义发生变化".into(),
            ));
        }
        let current_bindings = self
            .repo
            .bindings(&StandardConsumerLookup {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                model_id: current.id.clone(),
            })
            .await?;
        match (&claim.record.bindings, &current_bindings) {
            (None, None) if !has_declared_bindings(&current.properties) => {},
            (Some(before), Some(after))
                if before.binding_version == after.binding_version
                    && before.definition_hash == after.definition_hash =>
            {
                let dependencies = self
                    .validate_binding_set(&actor.tenant_id, &current.properties, after)
                    .await?;
                if dependencies != claim.record.dependencies {
                    return Err(AppError::Conflict("模型固定标准版本已变化".into()));
                }
            },
            _ => return Err(AppError::Conflict("模型实际标准绑定已变化".into())),
        }
        self.authorize(actor, &["models:read", "models:write", "models:execute"])
            .await?;
        Ok(())
    }
    pub async fn prepare_model_publication(
        &self,
        actor: &AuthIdentity,
        model_id: &str,
        expected_version: i64,
    ) -> Result<ModelStandardPublishClaim, AppError> {
        self.authorize(
            actor,
            &[
                "models:read",
                "models:write",
                "models:execute",
                "standards:read",
            ],
        )
        .await?;
        let resource = self
            .resource(actor, "data_model", model_id, expected_version)
            .await?;
        validation::model_properties(&resource.properties)?;
        let bindings = self
            .repo
            .bindings(&StandardConsumerLookup {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                model_id: model_id.into(),
            })
            .await?;
        let dependencies = if let Some(bindings) = &bindings {
            self.validate_binding_set(&actor.tenant_id, &resource.properties, bindings)
                .await?
        } else {
            if has_declared_bindings(&resource.properties) {
                return Err(AppError::Conflict(
                    "模型声明了标准引用但未通过真实标准绑定事务".into(),
                ));
            }
            Vec::new()
        };
        let record = self
            .repo
            .begin_model_publication(&BeginStandardModelPublicationParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                model_id: model_id.into(),
                expected_model_version: expected_version,
                expected_properties: resource.properties,
                binding_version: bindings.as_ref().map(|b| b.binding_version),
                binding_hash: bindings.as_ref().map(|b| b.definition_hash.clone()),
                dependencies,
            })
            .await?;
        if record.tenant_id != actor.tenant_id
            || record.actor_id != actor.sub
            || record.resource.tenant_id != actor.tenant_id
            || record.resource.id != model_id
            || record.resource.status != "publishing"
            || record.claim_version != record.resource.version
        {
            return Err(AppError::Internal(
                "模型发布事务门禁返回无效的实际占用".into(),
            ));
        }
        Ok(ModelStandardPublishClaim { record })
    }
    pub async fn finish_model_publication(
        &self,
        actor: &AuthIdentity,
        claim: &ModelStandardPublishClaim,
        outcome: StandardModelEngineOutcome,
    ) -> Result<StandardModelPublicationResult, AppError> {
        if actor.tenant_id != claim.record.tenant_id || actor.sub != claim.record.actor_id {
            return Err(AppError::Forbidden("实际模型发布占用不属于当前身份".into()));
        }
        let outcome = if let StandardModelEngineOutcome::Published { receipt } = &outcome {
            // No untrusted JSON can create this opaque claim. Still verify the
            // engine evidence before attempting a successful control-plane commit.
            if verify_physical_receipt(&claim.record.resource.properties, receipt).is_err() {
                StandardModelEngineOutcome::Unknown {
                    reason: "引擎报告了发布结果，但实际物理结构回执未通过模型核验".into(),
                    receipt: Some(receipt.clone()),
                }
            } else {
                outcome
            }
        } else {
            outcome
        };
        // Repository rechecks current identity/permissions and every pinned
        // revision. Lost authority after a physical effect is recorded as
        // unknown/invalidated, not silently discarded or presented as rollback.
        let result = self
            .repo
            .finish_model_publication(&FinishStandardModelPublicationParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                gate_id: claim.record.gate_id.clone(),
                expected_claim_version: claim.record.claim_version,
                outcome,
            })
            .await?;
        self.authorize(actor, &["models:read"]).await?;
        Ok(result)
    }
}
pub(super) fn has_declared_bindings(properties: &Value) -> bool {
    properties.get("naming_standard_id").is_some()
        || properties.get("naming_standard_version").is_some()
        || properties.get("standard_coverage").is_some()
        || properties["columns"].as_array().is_some_and(|columns| {
            columns.iter().any(|column| {
                [
                    "standard_id",
                    "standard_version",
                    "naming_standard_id",
                    "naming_standard_version",
                ]
                .iter()
                .any(|key| column.get(key).is_some())
            })
        })
}
pub(super) fn verify_physical_receipt(properties: &Value, receipt: &Value) -> Result<(), AppError> {
    if receipt["generated_ddl"].as_str().is_none_or(str::is_empty)
        || !receipt["reconciled"].is_boolean()
    {
        return Err(AppError::Conflict("引擎发布回执缺少实际执行身份".into()));
    }
    let physical = &receipt["physical_binding"];
    let table = properties["table_name"]
        .as_str()
        .ok_or_else(|| validation::bad("模型表名缺失"))?;
    let (schema, name) = table.split_once('.').unwrap_or(("main", table));
    if physical["schema_name"] != schema
        || physical["table_name"] != name
        || physical["table_type"] != "BASE TABLE"
        || physical["table_catalog"].as_str().is_none_or(|name| {
            name.is_empty() || name.len() > 1024 || name.chars().any(char::is_control)
        })
    {
        return Err(AppError::Conflict("实际物理对象身份与模型不一致".into()));
    }
    let expected = properties["columns"]
        .as_array()
        .ok_or_else(|| validation::bad("模型字段缺失"))?;
    let actual = receipt["physical_columns"]
        .as_array()
        .ok_or_else(|| AppError::Conflict("物理字段回执缺失".into()))?;
    if actual.len() != expected.len() {
        return Err(AppError::Conflict("物理字段数量与模型不一致".into()));
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let expected_type = crate::services::product_domain::warehouse::canonical_data_type(
            expected["data_type"].as_str().unwrap_or_default(),
        )?;
        let actual_type = crate::services::product_domain::warehouse::canonical_data_type(
            actual["data_type"].as_str().unwrap_or_default(),
        )?;
        let nullable = match actual["is_nullable"].as_str() {
            Some("YES") => true,
            Some("NO") => false,
            _ => return Err(AppError::Conflict("物理字段可空回执无效".into())),
        };
        if expected["name"] != actual["column_name"]
            || expected_type != actual_type
            || validation::effective_nullable(properties, expected) != nullable
        {
            return Err(AppError::Conflict("物理字段定义与模型不一致".into()));
        }
    }
    Ok(())
}
