use super::{ModelStandardsRequest, StandardGovernanceService, validation};
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::repo::standard_governance::*;
use serde_json::{Value, json};
use std::collections::HashSet;

impl StandardGovernanceService {
    pub async fn bindings(
        &self,
        actor: &AuthIdentity,
        model_id: &str,
    ) -> Result<Option<ModelStandardBindingSet>, AppError> {
        self.authorize(actor, &["models:read", "standards:read"])
            .await?;
        validation::identifier(model_id)?;
        let result = self
            .repo
            .bindings(&StandardConsumerLookup {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                model_id: model_id.into(),
            })
            .await?;
        self.authorize(actor, &["models:read", "standards:read"])
            .await?;
        Ok(result)
    }
    pub async fn bind_model(
        &self,
        actor: &AuthIdentity,
        model_id: &str,
        request: ModelStandardsRequest,
    ) -> Result<BindModelStandardsResult, AppError> {
        self.authorize(actor, &["models:write", "standards:read"])
            .await?;
        let model = self
            .resource(
                actor,
                "data_model",
                model_id,
                request.expected_model_version,
            )
            .await?;
        if request.columns.is_empty() && request.table_naming.is_none() {
            return Err(validation::bad(
                "标准绑定至少需要一个真实字段或表命名标准；空配置不能证明通过标准",
            ));
        }
        validation::model_properties(&model.properties)?;
        if !matches!(
            model.status.as_str(),
            "draft" | "publish_failed" | "published"
        ) || (model.status == "published" && !request.upgrade)
        {
            return Err(AppError::Conflict(
                "已发布模型更换标准须显式升级并回到草稿重新物理发布".into(),
            ));
        }
        let mut properties = model.properties.clone();
        let source_columns = model.properties["columns"]
            .as_array()
            .ok_or_else(|| validation::bad("模型字段缺失"))?;
        if request.columns.len() > source_columns.len() {
            return Err(validation::bad("标准绑定字段超出模型范围"));
        }
        let mut bindings = Vec::new();
        let mut seen = HashSet::new();
        for requested in &request.columns {
            if !seen.insert(requested.column_name.as_str()) {
                return Err(validation::bad("标准绑定字段重复"));
            }
            let column = source_columns
                .iter()
                .find(|column| column["name"].as_str() == Some(&requested.column_name))
                .ok_or_else(|| validation::bad("标准绑定必须引用已定义的精确模型字段名"))?;
            if requested.field_standard.is_none() && requested.naming_standard.is_none() {
                return Err(validation::bad("绑定项至少需要字段标准或命名标准"));
            }
            let field_standard = if let Some(reference) = &requested.field_standard {
                let standard = self
                    .active_revision(
                        &actor.tenant_id,
                        "field_standard",
                        &reference.id,
                        reference.version,
                    )
                    .await?;
                validation::check_field(&model.properties, column, &standard)?;
                Some(standard.reference())
            } else {
                None
            };
            let naming_standard = if let Some(reference) = &requested.naming_standard {
                let standard = self
                    .active_revision(
                        &actor.tenant_id,
                        "naming_standard",
                        &reference.id,
                        reference.version,
                    )
                    .await?;
                validation::check_name(&standard, "column", &requested.column_name)?;
                Some(standard.reference())
            } else {
                None
            };
            bindings.push(ModelColumnStandardBinding {
                column_name: requested.column_name.clone(),
                field_standard,
                naming_standard,
            });
        }
        // Preserve model order so identical bindings produce one deterministic hash.
        bindings.sort_by_key(|bound| {
            source_columns
                .iter()
                .position(|column| column["name"].as_str() == Some(&bound.column_name))
                .unwrap_or(usize::MAX)
        });
        if request.coverage == StandardCoverage::AllFields
            && source_columns.iter().any(|column| {
                !bindings.iter().any(|bound| {
                    column["name"].as_str() == Some(&bound.column_name)
                        && bound.field_standard.is_some()
                })
            })
        {
            return Err(validation::bad(
                "all_fields 覆盖要求每个模型字段都有已发布字段标准版本",
            ));
        }
        let table_naming = if let Some(reference) = &request.table_naming {
            let standard = self
                .active_revision(
                    &actor.tenant_id,
                    "naming_standard",
                    &reference.id,
                    reference.version,
                )
                .await?;
            let table_name = model.properties["table_name"]
                .as_str()
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default();
            validation::check_name(&standard, "table", table_name)?;
            Some(standard.reference())
        } else {
            None
        };
        project_binding_properties(&mut properties, request.coverage, &bindings, &table_naming)?;
        Ok(self
            .repo
            .bind_model(&BindModelStandardsParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                model_id: model_id.into(),
                expected_model_version: model.version,
                expected_binding_version: request.expected_binding_version,
                expected_properties: model.properties,
                properties,
                coverage: request.coverage,
                columns: bindings,
                table_naming,
                upgrade: request.upgrade,
            })
            .await?)
    }
    pub(super) async fn validate_binding_set(
        &self,
        tenant: &str,
        properties: &Value,
        bindings: &ModelStandardBindingSet,
    ) -> Result<Vec<FrozenStandardRef>, AppError> {
        validation::model_properties(properties)?;
        if bindings.tenant_id != tenant
            || bindings.model_properties_hash != standard_hash(properties)?
            || bindings.definition_hash
                != binding_hash(
                    bindings.coverage,
                    &bindings.columns,
                    &bindings.table_naming,
                    &bindings.model_properties_hash,
                )?
        {
            return Err(AppError::Conflict(
                "模型字段或标准绑定已改变，需显式重绑".into(),
            ));
        }
        let columns = properties["columns"]
            .as_array()
            .ok_or_else(|| validation::bad("模型字段缺失"))?;
        let mut dependencies = Vec::new();
        let mut seen = HashSet::new();
        for bound in &bindings.columns {
            if !seen.insert(bound.column_name.as_str()) {
                return Err(AppError::Conflict("标准绑定字段重复".into()));
            }
            let column = columns
                .iter()
                .find(|column| column["name"].as_str() == Some(&bound.column_name))
                .ok_or_else(|| AppError::Conflict("绑定字段已从模型移除".into()))?;
            for reference in [&bound.field_standard, &bound.naming_standard]
                .into_iter()
                .flatten()
            {
                let standard = self
                    .active_revision(
                        tenant,
                        &reference.kind,
                        &reference.id,
                        reference.revision_version,
                    )
                    .await?;
                if standard.reference() != *reference {
                    return Err(AppError::Conflict("绑定标准版本证据不一致".into()));
                }
                if reference.kind == "field_standard" {
                    validation::check_field(properties, column, &standard)?;
                } else if reference.kind == "naming_standard" {
                    validation::check_name(&standard, "column", &bound.column_name)?;
                } else {
                    return Err(AppError::Conflict("字段绑定标准类型无效".into()));
                }
                dependencies.push(reference.clone());
                dependencies.extend(standard.children);
            }
        }
        if bindings.coverage == StandardCoverage::AllFields
            && columns.iter().any(|column| {
                !bindings.columns.iter().any(|b| {
                    column["name"].as_str() == Some(&b.column_name) && b.field_standard.is_some()
                })
            })
        {
            return Err(AppError::Conflict("全字段覆盖不完整".into()));
        }
        if let Some(reference) = &bindings.table_naming {
            let standard = self
                .active_revision(
                    tenant,
                    &reference.kind,
                    &reference.id,
                    reference.revision_version,
                )
                .await?;
            if standard.reference() != *reference {
                return Err(AppError::Conflict("表命名标准版本证据不一致".into()));
            }
            validation::check_name(
                &standard,
                "table",
                properties["table_name"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('.')
                    .next()
                    .unwrap_or_default(),
            )?;
            dependencies.push(reference.clone());
        }
        let mut projected = properties.clone();
        project_binding_properties(
            &mut projected,
            bindings.coverage,
            &bindings.columns,
            &bindings.table_naming,
        )?;
        if projected != *properties {
            return Err(AppError::Conflict(
                "模型中的显式标准引用与真实绑定不一致".into(),
            ));
        }
        dependencies.sort_by(|a, b| {
            (&a.kind, &a.id, a.revision_version).cmp(&(&b.kind, &b.id, b.revision_version))
        });
        dependencies.dedup();
        Ok(dependencies)
    }
}
pub(super) fn project_binding_properties(
    properties: &mut Value,
    coverage: StandardCoverage,
    bindings: &[ModelColumnStandardBinding],
    table: &Option<FrozenStandardRef>,
) -> Result<(), AppError> {
    let columns = properties["columns"]
        .as_array_mut()
        .ok_or_else(|| validation::bad("模型字段缺失"))?;
    for column in columns {
        let name = column["name"].as_str().unwrap_or_default().to_owned();
        let map = column
            .as_object_mut()
            .ok_or_else(|| validation::bad("模型字段必须为对象"))?;
        for key in [
            "standard_id",
            "standard_version",
            "naming_standard_id",
            "naming_standard_version",
        ] {
            map.remove(key);
        }
        if let Some(binding) = bindings.iter().find(|bound| bound.column_name == name) {
            for (prefix, reference) in [
                ("standard", &binding.field_standard),
                ("naming_standard", &binding.naming_standard),
            ] {
                if let Some(reference) = reference {
                    map.insert(format!("{prefix}_id"), json!(reference.id));
                    map.insert(
                        format!("{prefix}_version"),
                        json!(reference.revision_version),
                    );
                }
            }
        }
    }
    let map = properties
        .as_object_mut()
        .ok_or_else(|| validation::bad("模型属性必须为对象"))?;
    for key in ["naming_standard_id", "naming_standard_version"] {
        map.remove(key);
    }
    if let Some(reference) = table {
        map.insert("naming_standard_id".into(), json!(reference.id));
        map.insert(
            "naming_standard_version".into(),
            json!(reference.revision_version),
        );
    }
    map.insert("standard_coverage".into(), json!(coverage));
    Ok(())
}
