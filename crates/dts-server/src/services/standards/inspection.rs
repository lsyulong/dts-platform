use super::{InspectModelStandardsRequest, StandardGovernanceService, validation};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product_domain::warehouse,
};
use chrono::Utc;
use dts_backend::repo::standard_governance::*;
use serde_json::{Value, json};

impl StandardGovernanceService {
    pub async fn inspect(
        &self,
        actor: &AuthIdentity,
        model_id: &str,
        request: InspectModelStandardsRequest,
    ) -> Result<StandardInspectionResult, AppError> {
        self.authorize(
            actor,
            &[
                "models:read",
                "standards:read",
                "standards:execute",
                "metadata:read",
            ],
        )
        .await?;
        validation::identifier(model_id)?;
        validation::identifier(&request.metadata_id)?;
        if [
            request.expected_model_version,
            request.expected_binding_version,
            request.metadata_version,
        ]
        .iter()
        .any(|v| *v < 1)
        {
            return Err(validation::bad("检查需要模型、绑定和实际元数据的精确版本"));
        }
        let source = StandardInspectionParams {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            model_id: model_id.into(),
            expected_model_version: request.expected_model_version,
            expected_binding_version: request.expected_binding_version,
            metadata_id: request.metadata_id,
            metadata_version: request.metadata_version,
        };
        let snapshot = self.repo.inspection_snapshot(&source).await?;
        let dependencies = self
            .validate_binding_set(
                &actor.tenant_id,
                &snapshot.resource.properties,
                &snapshot.bindings,
            )
            .await?;
        for revision in &snapshot.revisions {
            validation::verify_revision(revision, &actor.tenant_id)?;
        }
        let result = evaluate_snapshot(&snapshot, dependencies)?;
        Ok(self
            .repo
            .record_inspection(&RecordStandardInspectionParams {
                source,
                snapshot_hash: snapshot.snapshot_hash,
                result,
            })
            .await?)
    }
}
pub(super) fn evaluate_snapshot(
    snapshot: &StandardInspectionSnapshot,
    dependencies: Vec<FrozenStandardRef>,
) -> Result<StandardInspectionResult, AppError> {
    if snapshot.binding.engine != "swanlake"
        || snapshot.binding.tenant_id != snapshot.resource.tenant_id
        || snapshot.metadata.tenant_id != snapshot.resource.tenant_id
        || snapshot.binding.object_version != snapshot.metadata.version
    {
        return Err(AppError::Conflict(
            "采集元数据缺少当前租户真实引擎证明".into(),
        ));
    }
    let physical = &snapshot.publication.properties["receipt"]["physical_binding"];
    if physical["table_catalog"] != snapshot.binding.table_catalog
        || physical["schema_name"] != snapshot.binding.schema_name
        || physical["table_name"] != snapshot.binding.table_name
    {
        return Err(AppError::Conflict(
            "采集对象与实际模型发布的三段物理身份不一致".into(),
        ));
    }
    let model = &snapshot.resource.properties;
    let model_columns = model["columns"]
        .as_array()
        .ok_or_else(|| validation::bad("模型字段缺失"))?;
    let actual_columns = snapshot.metadata.properties["columns"]
        .as_array()
        .ok_or_else(|| AppError::Conflict("已采集元数据没有实际字段快照".into()))?;
    let mut issues = Vec::new();
    for expected in model_columns {
        let name = expected["name"]
            .as_str()
            .ok_or_else(|| validation::bad("模型字段名缺失"))?;
        let Some(actual) = actual_columns.iter().find(|column| column["name"] == name) else {
            issues.push(issue(
                Some(name),
                "missing_column",
                "采集快照缺少模型字段",
                Some(json!(name)),
                None,
            ));
            continue;
        };
        let expected_type =
            warehouse::canonical_data_type(expected["data_type"].as_str().unwrap_or_default())?;
        let actual_type =
            warehouse::canonical_data_type(actual["data_type"].as_str().unwrap_or_default())?;
        if expected_type != actual_type {
            issues.push(issue(
                Some(name),
                "data_type_mismatch",
                "采集字段类型与模型不一致",
                Some(json!(expected_type)),
                Some(json!(actual_type)),
            ));
        }
        let actual_nullable = actual["nullable"]
            .as_bool()
            .ok_or_else(|| AppError::Conflict("采集可空属性不是实际布尔值".into()))?;
        let expected_nullable = validation::effective_nullable(model, expected);
        if expected_nullable != actual_nullable {
            issues.push(issue(
                Some(name),
                "nullable_mismatch",
                "采集字段可空属性与模型不一致",
                Some(json!(expected_nullable)),
                Some(json!(actual_nullable)),
            ));
        }
        if let Some(bound) = snapshot
            .bindings
            .columns
            .iter()
            .find(|bound| bound.column_name == name)
        {
            if let Some(reference) = &bound.field_standard {
                let standard = exact_revision(snapshot, reference)?;
                if warehouse::canonical_data_type(
                    standard.properties["data_type"]
                        .as_str()
                        .unwrap_or_default(),
                )? != actual_type
                {
                    issues.push(issue(
                        Some(name),
                        "standard_type_mismatch",
                        "采集类型不符合固定字段标准",
                        Some(standard.properties["data_type"].clone()),
                        Some(json!(actual_type)),
                    ));
                }
                if standard.properties["nullable"] == false && actual_nullable {
                    issues.push(issue(
                        Some(name),
                        "standard_nullability_mismatch",
                        "采集字段允许空值，固定标准要求非空",
                        Some(json!(false)),
                        Some(json!(true)),
                    ));
                }
            }
            if let Some(reference) = &bound.naming_standard {
                if validation::check_name(exact_revision(snapshot, reference)?, "column", name)
                    .is_err()
                {
                    issues.push(issue(
                        Some(name),
                        "column_name_mismatch",
                        "采集字段名不符合固定命名标准",
                        None,
                        Some(json!(name)),
                    ));
                }
            }
        }
    }
    for actual in actual_columns {
        if !model_columns
            .iter()
            .any(|column| column["name"] == actual["name"])
        {
            issues.push(issue(
                actual["name"].as_str(),
                "unexpected_column",
                "采集快照包含模型未声明字段",
                None,
                Some(actual["name"].clone()),
            ));
        }
    }
    if let Some(reference) = &snapshot.bindings.table_naming {
        if validation::check_name(
            exact_revision(snapshot, reference)?,
            "table",
            &snapshot.binding.table_name,
        )
        .is_err()
        {
            issues.push(issue(
                None,
                "table_name_mismatch",
                "采集表名不符合固定命名标准",
                None,
                Some(json!(snapshot.binding.table_name)),
            ));
        }
    }
    Ok(StandardInspectionResult {
        id: uuid::Uuid::new_v4().to_string(),
        model_id: snapshot.resource.id.clone(),
        model_version: snapshot.resource.version,
        binding_version: snapshot.bindings.binding_version,
        metadata_id: snapshot.metadata.id.clone(),
        metadata_version: snapshot.metadata.version,
        metadata_observed_at: snapshot.metadata_observed_at,
        source_fingerprint: snapshot.binding.source_fingerprint.clone(),
        evaluation_scope: "collected_schema".into(),
        state: if issues.is_empty() {
            "conformant"
        } else {
            "non_conformant"
        }
        .into(),
        issues,
        data_values_evaluated: false,
        unit_semantics_evaluated: false,
        dependency_versions: dependencies,
        created_at: Utc::now(),
    })
}
fn exact_revision<'a>(
    snapshot: &'a StandardInspectionSnapshot,
    reference: &FrozenStandardRef,
) -> Result<&'a StandardRevision, AppError> {
    snapshot
        .revisions
        .iter()
        .find(|revision| revision.reference() == *reference)
        .ok_or_else(|| AppError::Conflict("检查快照缺少精确标准版本".into()))
}
fn issue(
    column: Option<&str>,
    code: &str,
    message: &str,
    expected: Option<Value>,
    actual: Option<Value>,
) -> StandardInspectionIssue {
    StandardInspectionIssue {
        column_name: column.map(str::to_owned),
        code: code.into(),
        message: message.into(),
        expected,
        actual,
    }
}
