use super::{SqliteStandardGovernanceRepo, authority, dependencies, storage::*};
use dts_backend::{BackendError, repo::standard_governance::*};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn stripped(mut properties: Value) -> Value {
    if let Some(object) = properties.as_object_mut() {
        object.remove("naming_standard_id");
        object.remove("naming_standard_version");
        object.remove("standard_coverage");
        if let Some(columns) = object.get_mut("columns").and_then(Value::as_array_mut) {
            for column in columns {
                if let Some(column) = column.as_object_mut() {
                    for key in [
                        "standard_id",
                        "standard_version",
                        "naming_standard_id",
                        "naming_standard_version",
                    ] {
                        column.remove(key);
                    }
                }
            }
        }
    }
    properties
}

pub(super) fn validate_properties(p: &BindModelStandardsParams) -> Result<(), BackendError> {
    if stripped(p.expected_properties.clone()) != stripped(p.properties.clone()) {
        return Err(invalid(
            "标准绑定只能修改模型标准引用，其他模型定义必须保持一致",
        ));
    }
    let columns = p
        .properties
        .get("columns")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 1000)
        .ok_or_else(|| invalid("模型必须有1至1000个结构化字段"))?;
    let mut actual = BTreeMap::new();
    for column in columns {
        let name = column
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("模型字段名称无效"))?;
        text(name)?;
        if actual.insert(name.to_lowercase(), column).is_some() {
            return Err(invalid("模型字段名称重复"));
        }
    }
    if p.columns.len() > 1000 || (p.columns.is_empty() && p.table_naming.is_none()) {
        return Err(invalid("模型标准绑定不能为空或超过1000项"));
    }
    let mut configured = BTreeSet::new();
    let ordered = p
        .columns
        .iter()
        .map(|binding| binding.column_name.as_str())
        .collect::<Vec<_>>();
    let actual_order = columns
        .iter()
        .filter_map(|column| column["name"].as_str())
        .filter(|name| ordered.contains(name))
        .collect::<Vec<_>>();
    if ordered != actual_order || p.properties["standard_coverage"] != serde_json::json!(p.coverage)
    {
        return Err(invalid(
            "标准绑定必须沿实际模型字段顺序且覆盖范围与模型一致",
        ));
    }
    for binding in &p.columns {
        let name = binding.column_name.to_lowercase();
        if !configured.insert(name.clone()) {
            return Err(invalid("同一模型字段不能重复绑定"));
        }
        let column = actual
            .get(&name)
            .ok_or_else(|| invalid("标准绑定引用的字段不在实际模型定义中"))?;
        if binding.field_standard.is_none() && binding.naming_standard.is_none() {
            return Err(invalid("字段绑定必须指定字段或命名标准修订"));
        }
        check_ref(
            column,
            "standard_id",
            "standard_version",
            &binding.field_standard,
            "field_standard",
        )?;
        check_ref(
            column,
            "naming_standard_id",
            "naming_standard_version",
            &binding.naming_standard,
            "naming_standard",
        )?;
    }
    for (name, column) in &actual {
        if !configured.contains(name) {
            check_ref(
                column,
                "standard_id",
                "standard_version",
                &None,
                "field_standard",
            )?;
            check_ref(
                column,
                "naming_standard_id",
                "naming_standard_version",
                &None,
                "naming_standard",
            )?;
        }
        if p.coverage == StandardCoverage::AllFields
            && !p
                .columns
                .iter()
                .any(|b| b.column_name.eq_ignore_ascii_case(name) && b.field_standard.is_some())
        {
            return Err(invalid("全部字段覆盖要求每个真实模型字段都绑定字段标准"));
        }
    }
    check_ref(
        &p.properties,
        "naming_standard_id",
        "naming_standard_version",
        &p.table_naming,
        "naming_standard",
    )
}
fn check_ref(
    properties: &Value,
    id_key: &str,
    version_key: &str,
    reference: &Option<FrozenStandardRef>,
    required_kind: &str,
) -> Result<(), BackendError> {
    let id = properties.get(id_key).filter(|v| !v.is_null());
    let revision = properties.get(version_key).filter(|v| !v.is_null());
    match reference {
        Some(reference)
            if reference.kind == required_kind
                && id.and_then(Value::as_str) == Some(reference.id.as_str())
                && revision.and_then(Value::as_i64) == Some(reference.revision_version) =>
        {
            Ok(())
        },
        None if id.is_none() && revision.is_none() => Ok(()),
        _ => Err(invalid("模型定义中的标准标识/修订与冻结绑定不一致")),
    }
}
pub(super) async fn bind(
    repo: &SqliteStandardGovernanceRepo,
    p: &BindModelStandardsParams,
) -> Result<BindModelStandardsResult, BackendError> {
    text(&p.model_id)?;
    version(p.expected_model_version)?;
    validate_properties(p)?;
    let mut tx = repo.begin_write(&p.tenant_id).await?;
    authority::require(
        &mut tx,
        &p.tenant_id,
        &p.actor_id,
        &["models:write", "standards:read"],
    )
    .await?;
    let before = resource(&mut tx, &p.tenant_id, "data_model", &p.model_id).await?;
    if before.version != p.expected_model_version
        || before.properties != p.expected_properties
        || !(before.status == "draft" || (before.status == "published" && p.upgrade))
    {
        return Err(conflict("模型必须是当前草稿，或明确升级已发布模型为新草稿"));
    }
    let current = binding(&mut tx, &p.tenant_id, &p.model_id).await?;
    if current.as_ref().map(|b| b.binding_version) != p.expected_binding_version {
        return Err(conflict("模型标准绑定版本已改变"));
    }
    let next_version = current.as_ref().map_or(Ok(1), |b| {
        b.binding_version
            .checked_add(1)
            .ok_or_else(|| conflict("标准绑定版本已耗尽"))
    })?;
    let model_properties_hash = standard_hash(&p.properties)?;
    let mut bindings = ModelStandardBindingSet {
        tenant_id: p.tenant_id.clone(),
        model_id: p.model_id.clone(),
        model_version: before.version + 1,
        binding_version: next_version,
        coverage: p.coverage,
        columns: p.columns.clone(),
        table_naming: p.table_naming.clone(),
        definition_hash: binding_hash(
            p.coverage,
            &p.columns,
            &p.table_naming,
            &model_properties_hash,
        )?,
        model_properties_hash,
        created_by: p.actor_id.clone(),
        created_at: chrono::Utc::now(),
    };
    let revisions = dependencies::for_binding(&mut tx, &p.tenant_id, &bindings).await?;
    let resource = write(&mut tx, &before, &p.actor_id, p.properties.clone(), "draft").await?;
    bindings.created_at = resource.updated_at;
    let encoded = encode(&bindings)?;
    if encoded.len() > 262144 {
        return Err(invalid("模型标准绑定超过256KiB"));
    }
    sqlx::query("INSERT INTO standard_model_binding(tenant_id,model_id,binding_version,payload,created_by) VALUES(?1,?2,?3,?4,?5)").bind(&p.tenant_id).bind(&p.model_id).bind(next_version).bind(encoded).bind(&p.actor_id).execute(&mut *tx).await.map_err(error)?;
    for revision in revisions {
        sqlx::query("INSERT INTO standard_model_binding_revision(tenant_id,model_id,binding_version,kind,id,revision_version) VALUES(?1,?2,?3,?4,?5,?6)").bind(&p.tenant_id).bind(&p.model_id).bind(next_version).bind(&revision.kind).bind(&revision.id).bind(revision.revision_version).execute(&mut *tx).await.map_err(error)?;
    }
    sqlx::query("INSERT INTO standard_model_current(tenant_id,model_id,binding_version) VALUES(?1,?2,?3) ON CONFLICT(tenant_id,model_id) DO UPDATE SET binding_version=excluded.binding_version").bind(&p.tenant_id).bind(&p.model_id).bind(next_version).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(BindModelStandardsResult { resource, bindings })
}
