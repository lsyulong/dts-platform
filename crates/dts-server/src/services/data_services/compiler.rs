// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, schema, sql, types::*};
use crate::{models::error::AppError, services::product_jobs::parameters};
use dts_backend::repo::{
    metadata_catalog::binding_matches,
    product_job::{ProductJob, ProductJobNodeDefinition},
};
use serde_json::{Value, json};

pub(super) fn definition(
    properties: &Value,
    metadata: &ApiMetadata,
) -> Result<ApiDefinition, AppError> {
    let source_sql = properties["sql"]
        .as_str()
        .ok_or_else(|| bad("数据 API 缺少明细查询 SQL"))?;
    let parameter_value = properties
        .get("parameters")
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let parameters: Vec<parameters::ParameterDefinition> =
        serde_json::from_value(parameter_value.clone())
            .map_err(|_| bad("API 请求参数不符合类型化声明合同"))?;
    parameters::validate_definitions(&parameters)?;
    let row_limit = positive(properties, "row_limit", 1000)?;
    let rate_limit = positive(properties, "rate_limit", 100000)?;
    let compiled = sql::compile(source_sql, metadata, row_limit)?;
    let used = parameters::inspect(&compiled.sql, &parameters)?;
    if parameters
        .iter()
        .any(|parameter| !used.contains(&parameter.name))
    {
        return Err(bad("API 声明了未使用的参数"));
    }
    let response_schema = schema::declared(properties, compiled.schema)?;
    Ok(ApiDefinition {
        source_sql: source_sql.into(),
        compiled_sql: compiled.sql,
        parameters: parameter_value,
        response_schema,
        row_limit,
        rate_limit,
    })
}

fn positive(properties: &Value, key: &str, max: u32) -> Result<u32, AppError> {
    properties[key]
        .as_u64()
        .filter(|value| *value > 0 && *value <= u64::from(max))
        .map(|value| value as u32)
        .ok_or_else(|| bad(&format!("{key} 必须为 1 至 {max} 的整数")))
}

pub(super) fn publication(
    properties: &Value,
    tenant: &str,
    id: &str,
    version: i64,
) -> Result<ApiPublication, AppError> {
    let publication: ApiPublication = serde_json::from_value(properties["publication"].clone())
        .map_err(|_| bad("API 缺少服务端发布快照，请重新校验发布"))?;
    let metadata = &publication.metadata;
    if publication.format_version != 1
        || publication.executor != "data_api_detail_v1"
        || publication.tenant_id != tenant
        || publication.source_id != id
        || publication.source_version != version
        || publication.published_by.is_empty()
        || chrono::DateTime::parse_from_rfc3339(&publication.published_at).is_err()
        || metadata.binding.engine != "swanlake"
        || metadata.binding.tenant_id != tenant
        || metadata.version != metadata.binding.object_version
        || properties["metadata_object_id"] != metadata.id
        || metadata.properties["columns"] != json!(metadata.columns)
        || !binding_matches(&metadata.binding, &metadata.properties)
    {
        return Err(bad("API 发布快照与租户、版本、实际元数据来源不一致"));
    }
    if definition(properties, metadata)? != publication.api {
        return Err(bad("API 当前定义与已发布冻结定义不一致"));
    }
    Ok(publication)
}

pub(super) fn bind(api: &ApiDefinition, input: &Value) -> Result<(Value, String), AppError> {
    let declarations =
        serde_json::from_value::<Vec<parameters::ParameterDefinition>>(api.parameters.clone())
            .map_err(|_| bad("冻结 API 参数声明无效"))?;
    let (parameters, expressions) = parameters::resolve(&declarations, input)?;
    Ok((
        parameters,
        parameters::bind(&api.compiled_sql, &expressions)?,
    ))
}

pub(super) fn node(publication: &ApiPublication) -> ProductJobNodeDefinition {
    ProductJobNodeDefinition {
        id: "api".into(),
        name: "执行真实数据 API 明细查询".into(),
        definition: json!({"format_version":1,"type":"data_api_detail",
        "source_version":publication.source_version,"source_sql":publication.api.compiled_sql,"parameters":publication.api.parameters,
        "response_schema":publication.api.response_schema,"row_limit":publication.api.row_limit,"metadata_id":publication.metadata.id,"metadata_version":publication.metadata.version}),
        depends_on: vec![],
    }
}

pub(super) fn snapshot(job: &ProductJob) -> Result<ApiJobSnapshot, AppError> {
    let frozen: ApiJobSnapshot = serde_json::from_value(job.definition_snapshot.clone())
        .map_err(|_| bad("API 运行快照不符合固定合同"))?;
    if !job.can_retry
        || job.action != "execute_data_api"
        || job.source_kind != "data_api"
        || frozen.format_version != 1
        || frozen.executor != "data_api_detail_v1"
        || frozen.tenant_id != job.tenant_id
        || frozen.source_kind != job.source_kind
        || frozen.source_id != job.source_id
        || frozen.source_version != job.source_version
        || frozen.publication
            != publication(
                &frozen.properties,
                &job.tenant_id,
                &job.source_id,
                job.source_version,
            )?
        || frozen.metadata != frozen.publication.metadata
    {
        return Err(bad("API 运行身份、服务端发布版本或元数据快照不一致"));
    }
    Ok(frozen)
}

pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let frozen = snapshot(job)?;
    let expected = node(&frozen.publication);
    if job.nodes.len() != 1
        || job.nodes[0].id != expected.id
        || job.nodes[0].name != expected.name
        || job.nodes[0].definition != expected.definition
        || job.nodes[0].depends_on != expected.depends_on
    {
        return Err(bad("API 执行节点与已发布服务端定义不一致"));
    }
    let (parameters, sql) = bind(&frozen.publication.api, &job.parameters)?;
    if parameters != job.parameters {
        return Err(bad("API 运行参数没有规范化绑定"));
    }
    Ok(vec![(expected.id, sql)])
}
