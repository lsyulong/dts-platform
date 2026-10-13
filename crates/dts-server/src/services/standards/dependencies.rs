use super::validation;
use crate::models::error::AppError;
use dts_backend::{ProductResource, repo::standard_governance::*};
use serde_json::Value;

pub(crate) async fn active_revision(
    repo: &dyn StandardGovernanceRepo,
    tenant: &str,
    kind: &str,
    id: &str,
    version: i64,
) -> Result<StandardRevision, AppError> {
    validation::standard_kind(kind)?;
    validation::identifier(id)?;
    if version < 1 {
        return Err(validation::bad("固定标准版本必须为正整数"));
    }
    let revision = repo
        .revision(tenant, kind, id, version)
        .await?
        .ok_or_else(|| AppError::Conflict("没有已发布的固定标准版本，草稿不能作为依赖".into()))?;
    validation::verify_revision(&revision, tenant)?;
    for child in &revision.children {
        let dependency = repo
            .revision(tenant, &child.kind, &child.id, child.revision_version)
            .await?
            .ok_or_else(|| AppError::Conflict("固定标准子版本不存在".into()))?;
        validation::verify_revision(&dependency, tenant)?;
        if dependency.reference() != *child || !dependency.children.is_empty() {
            return Err(AppError::Conflict("固定标准子版本证据不一致".into()));
        }
    }
    Ok(revision)
}

/// Called at every controlled execution/result-read boundary. Only actual
/// immutable revisions and the real publication receipt can satisfy a binding.
pub(crate) async fn model_dependencies(
    repo: Option<&dyn StandardGovernanceRepo>,
    tenant: &str,
    model: &ProductResource,
    publication: &ProductResource,
) -> Result<Vec<FrozenStandardRef>, AppError> {
    if model.tenant_id != tenant
        || publication.tenant_id != tenant
        || model.status != "published"
        || publication.kind != "model_publication"
        || publication.status != "published"
    {
        return Err(AppError::Forbidden(
            "模型标准依赖不属于当前真实发布模型".into(),
        ));
    }
    if !super::gate::has_declared_bindings(&model.properties) {
        // Legacy physically published models remain usable without inventing a
        // standards pass. Every new gate receipt explicitly names not_configured.
        if publication.properties.get("standards").is_some()
            && publication.properties["standards"]["state"] != "not_configured"
        {
            return Err(AppError::Conflict(
                "模型未配置标准但回执声明了其它标准状态".into(),
            ));
        }
        return Ok(Vec::new());
    }
    let repo =
        repo.ok_or_else(|| AppError::Forbidden("模型配置了标准，但不可变修订仓储未配置".into()))?;
    validation::model_properties(&model.properties)?;
    let coverage = match model.properties["standard_coverage"].as_str() {
        Some("bound_fields") => StandardCoverage::BoundFields,
        Some("all_fields") => StandardCoverage::AllFields,
        _ => return Err(AppError::Conflict("模型标准覆盖范围未通过绑定事务".into())),
    };
    let mut columns = Vec::new();
    let mut dependencies = Vec::new();
    for column in model.properties["columns"]
        .as_array()
        .ok_or_else(|| validation::bad("模型字段缺失"))?
    {
        let name = column["name"]
            .as_str()
            .ok_or_else(|| validation::bad("模型字段名缺失"))?;
        let field = paired(column, "standard")?;
        let naming = paired(column, "naming_standard")?;
        let field_standard = if let Some((id, version)) = field {
            let standard = active_revision(repo, tenant, "field_standard", id, version).await?;
            validation::check_field(&model.properties, column, &standard)?;
            dependencies.extend(standard.children.clone());
            dependencies.push(standard.reference());
            Some(standard.reference())
        } else {
            None
        };
        let naming_standard = if let Some((id, version)) = naming {
            let standard = active_revision(repo, tenant, "naming_standard", id, version).await?;
            validation::check_name(&standard, "column", name)?;
            dependencies.push(standard.reference());
            Some(standard.reference())
        } else {
            None
        };
        if coverage == StandardCoverage::AllFields && field_standard.is_none() {
            return Err(AppError::Conflict("模型全字段标准覆盖不完整".into()));
        }
        if field_standard.is_some() || naming_standard.is_some() {
            columns.push(ModelColumnStandardBinding {
                column_name: name.into(),
                field_standard,
                naming_standard,
            });
        }
    }
    let table_naming = if let Some((id, version)) = paired(&model.properties, "naming_standard")? {
        let standard = active_revision(repo, tenant, "naming_standard", id, version).await?;
        validation::check_name(
            &standard,
            "table",
            model.properties["table_name"]
                .as_str()
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default(),
        )?;
        dependencies.push(standard.reference());
        Some(standard.reference())
    } else {
        None
    };
    dependencies.sort_by(|a, b| {
        (&a.kind, &a.id, a.revision_version).cmp(&(&b.kind, &b.id, b.revision_version))
    });
    dependencies.dedup();
    if dependencies.is_empty() {
        return Err(AppError::Conflict(
            "空标准声明不能证明实际发布时通过了标准门禁".into(),
        ));
    }
    let proof = &publication.properties["standards"];
    let actual_hash = binding_hash(
        coverage,
        &columns,
        &table_naming,
        &standard_hash(&model.properties)?,
    )?;
    let frozen: Vec<FrozenStandardRef> = serde_json::from_value(proof["dependencies"].clone())
        .map_err(|_| AppError::Conflict("实际模型发布缺少固定标准依赖".into()))?;
    if proof["state"] != "validated"
        || proof["binding_version"].as_i64().is_none_or(|v| v < 1)
        || proof["binding_hash"] != actual_hash
        || frozen != dependencies
    {
        return Err(AppError::Conflict(
            "实际模型标准发布证明与当前固定修订不一致".into(),
        ));
    }
    Ok(dependencies)
}
fn paired<'a>(properties: &'a Value, prefix: &str) -> Result<Option<(&'a str, i64)>, AppError> {
    let id_key = format!("{prefix}_id");
    let version_key = format!("{prefix}_version");
    match (properties.get(&id_key), properties.get(&version_key)) {
        (None, None) => Ok(None),
        (Some(id), Some(version)) => {
            let id = id
                .as_str()
                .ok_or_else(|| validation::bad("标准标识必须为字符串"))?;
            validation::identifier(id)?;
            let version = version
                .as_i64()
                .filter(|v| *v > 0)
                .ok_or_else(|| validation::bad("固定标准版本必须为正整数"))?;
            Ok(Some((id, version)))
        },
        _ => Err(validation::bad(
            "标准标识和版本必须成对，不能引用未发布草稿",
        )),
    }
}
