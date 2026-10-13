use super::{bad, definition::quote, expression::column, types::*};
use crate::{models::error::AppError, services::product_domain::warehouse::canonical_data_type};
use dts_backend::{
    ProductResource, ProductResourceFilter, ProductResourceRepo,
    repo::metadata_catalog::{MetadataCatalogRepo, MetadataColumn, binding_matches},
};

pub(super) async fn get(
    resources: &dyn ProductResourceRepo,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, AppError> {
    resources
        .get(tenant, kind, id)
        .await?
        .ok_or_else(|| AppError::NotFound("指标依赖不存在于当前租户".into()))
}
pub(super) async fn metadata(
    resources: &dyn ProductResourceRepo,
    registry: &dyn MetadataCatalogRepo,
    tenant: &str,
    id: &str,
) -> Result<FrozenMetadata, AppError> {
    let resource = get(resources, tenant, "metadata_object", id).await?;
    let binding = registry
        .verified_binding(tenant, id, resource.version)
        .await?
        .filter(|binding| {
            binding.engine == "swanlake"
                && binding.tenant_id == tenant
                && binding_matches(binding, &resource.properties)
        })
        .ok_or_else(|| bad("指标必须绑定具有当前真实SwanLake采集证明的物理表"))?;
    let columns: Vec<MetadataColumn> =
        serde_json::from_value(resource.properties["columns"].clone())
            .map_err(|_| bad("指标实际元数据字段无效"))?;
    if columns.is_empty()
        || columns.len() > 1000
        || resource.properties["object_type"].as_str() != Some("table")
    {
        return Err(bad("指标当前仅支持实际采集的基础表"));
    }
    Ok(FrozenMetadata {
        id: resource.id,
        version: resource.version,
        properties: resource.properties,
        binding,
        columns,
    })
}
pub(super) async fn model_receipt(
    resources: &dyn ProductResourceRepo,
    tenant: &str,
    model: &ProductResource,
) -> Result<ProductResource, AppError> {
    if model.status != "published" {
        return Err(bad("指标只能绑定已经物理发布的模型"));
    }
    let mut found = None;
    for offset in (0..1000).step_by(100) {
        let page = resources
            .list(&ProductResourceFilter {
                tenant_id: tenant.into(),
                kind: "model_publication".into(),
                q: None,
                status: Some("published".into()),
                created_by: None,
                limit: 100,
                offset,
            })
            .await?;
        if page.total > 1000 {
            return Err(bad(
                "模型发布历史超过当前核验容量，请使用专属发布索引后再运行",
            ));
        }
        for receipt in page.data {
            if receipt.properties["model_version"].as_i64() == Some(model.version - 1)
                && receipt.references.iter().any(|reference| {
                    reference.target_kind == "data_model"
                        && reference.target_id == model.id
                        && reference.field == "model_id"
                })
            {
                if found.is_some() {
                    return Err(bad("模型物理发布证明不唯一"));
                }
                found = Some(receipt);
            }
        }
        if i64::from(offset) + 100 >= page.total {
            break;
        }
    }
    found.ok_or_else(|| bad("当前模型版本没有实际物理发布回执，不能仅凭状态运行指标"))
}
pub(super) fn model_matches(
    model: &ProductResource,
    receipt: &ProductResource,
    metadata: &FrozenMetadata,
    definition: &AtomicDefinition,
) -> Result<(), AppError> {
    let table = model.properties["table_name"]
        .as_str()
        .ok_or_else(|| bad("物理模型缺少表名"))?;
    let (schema, table) = table.split_once('.').unwrap_or(("main", table));
    let physical_binding = &receipt.properties["receipt"]["physical_binding"];
    if schema != metadata.binding.schema_name
        || table != metadata.binding.table_name
        || physical_binding["table_catalog"].as_str()
            != Some(metadata.binding.table_catalog.as_str())
        || physical_binding["schema_name"].as_str() != Some(schema)
        || physical_binding["table_name"].as_str() != Some(table)
        || physical_binding["table_type"].as_str() != Some("BASE TABLE")
    {
        return Err(bad(
            "实际元数据定位与物理模型发布命名空间证明不一致或证明缺失",
        ));
    }
    let keys: Vec<String> = serde_json::from_value(model.properties["primary_key"].clone())
        .map_err(|_| bad("模型主键无效"))?;
    if keys.is_empty()
        || keys.len() != definition.grain_columns.len()
        || keys
            .iter()
            .zip(&definition.grain_columns)
            .any(|(a, b)| !a.eq_ignore_ascii_case(b))
    {
        return Err(bad("指标粒度字段须按顺序匹配已物理核验的模型主键"));
    }
    let model_columns = model.properties["columns"]
        .as_array()
        .ok_or_else(|| bad("物理模型字段缺失"))?;
    let physical = receipt.properties["receipt"]["physical_columns"]
        .as_array()
        .ok_or_else(|| bad("发布回执缺少实际字段证明"))?;
    if physical.len() != model_columns.len() || physical.len() != metadata.columns.len() {
        return Err(bad("模型、发布回执与实际采集字段数量不一致"));
    }
    for ((expected, physical), actual) in model_columns.iter().zip(physical).zip(&metadata.columns)
    {
        let name = expected["name"]
            .as_str()
            .ok_or_else(|| bad("模型字段名缺失"))?;
        let ty = canonical_data_type(expected["data_type"].as_str().unwrap_or_default())?;
        let nullable = expected["nullable"].as_bool().unwrap_or(true)
            && !keys.iter().any(|key| key.eq_ignore_ascii_case(name));
        if name != actual.name
            || physical["column_name"].as_str() != Some(name)
            || ty != canonical_data_type(&actual.data_type)?
            || ty != canonical_data_type(physical["data_type"].as_str().unwrap_or_default())?
            || nullable != actual.nullable
            || nullable != (physical["is_nullable"].as_str() == Some("YES"))
        {
            return Err(bad("实际物理字段、类型或非空约束与模型发布证明已不一致"));
        }
    }
    let time = column(&definition.time_column, &metadata.columns)?;
    if !matches!(
        canonical_data_type(&time.data_type)?.as_str(),
        "DATE" | "TIMESTAMP" | "TIMESTAMPTZ"
    ) {
        return Err(bad("数据窗时间字段须为实际DATE/TIMESTAMP/TIMESTAMPTZ"));
    }
    dimensions(&definition.dimensions, &metadata.columns)?;
    for key in &keys {
        if column(key, &metadata.columns)?.nullable {
            return Err(bad("指标粒度主键必须实际非空"));
        }
    }
    Ok(())
}
pub(super) fn dimensions(
    names: &[String],
    columns: &[MetadataColumn],
) -> Result<Vec<String>, AppError> {
    names
        .iter()
        .map(|name| {
            let column = column(name, columns)?;
            quote(&column.name)?;
            if ["metric_value", "input_rows"]
                .iter()
                .any(|reserved| column.name.eq_ignore_ascii_case(reserved))
            {
                return Err(bad("维度不能覆盖指标实际结果保留字段"));
            }
            if matches!(
                canonical_data_type(&column.data_type)?.as_str(),
                "JSON" | "BLOB"
            ) {
                return Err(bad("指标分组维度不能是JSON或二进制字段"));
            }
            Ok(column.name.clone())
        })
        .collect()
}
pub(super) fn publication(resource: &ProductResource) -> Result<MetricPublication, AppError> {
    if resource.status != "published" {
        return Err(bad("依赖指标、修饰词或周期尚未发布或已撤回"));
    }
    let value: MetricPublication =
        serde_json::from_value(resource.properties["publication"].clone())
            .map_err(|_| bad("指标缺少服务端冻结发布证明"))?;
    if value.format_version != 1
        || value.executor != "metric_aggregate_v1"
        || value.tenant_id != resource.tenant_id
        || value.source_kind != resource.kind
        || value.source_id != resource.id
        || value.source_version != resource.version
        || value.properties != super::definition::definition_properties(&resource.properties)?
    {
        return Err(bad("指标发布版本或定义已改变，须重新发布"));
    }
    Ok(value)
}
