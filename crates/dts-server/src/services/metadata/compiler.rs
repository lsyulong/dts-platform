// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::bad;
use crate::{models::error::AppError, services::product_jobs::parameters::bind_optional_text};
use dts_backend::repo::{
    metadata_catalog::MetadataScope,
    product_job::{ProductJob, ProductJobNodeDefinition},
};
use serde_json::{Value, json};

pub(super) fn scope(properties: &Value) -> Result<MetadataScope, AppError> {
    match properties["source_type"].as_str() {
        Some("swanlake") => {},
        Some("datasource") => return Err(bad("该数据源尚未配置真实元数据采集驱动，无法采集")),
        _ => return Err(bad("元数据来源必须明确为 swanlake 或 datasource")),
    }
    if properties
        .get("datasource_id")
        .is_some_and(|value| !value.is_null() && value.as_str() != Some(""))
    {
        return Err(bad("SwanLake 来源不能同时声明任意数据源关联"));
    }
    if properties.get("engine_binding").is_some() {
        return Err(bad("来源证明只能由采集仓储产生"));
    }
    Ok(MetadataScope {
        engine: "swanlake".into(),
        table_catalog: filter(properties, "catalog_filter")?,
        schema_name: filter(properties, "schema_filter")?,
        table_name: filter(properties, "table_filter")?,
    })
}

fn filter(properties: &Value, key: &str) -> Result<Option<String>, AppError> {
    let Some(value) = properties.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| bad("采集范围必须为单个精确名称或空字符串"))?;
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(bad("采集范围名称过长或包含控制字符"));
    }
    Ok((!value.trim().is_empty()).then(|| value.into()))
}

pub(super) fn query(scope: &MetadataScope) -> Result<String, AppError> {
    if scope.engine != "swanlake" {
        return Err(bad("采集引擎不是受控 SwanLake"));
    }
    let sql = concat!(
        "SELECT t.table_catalog AS table_catalog, t.table_schema AS schema_name, t.table_name AS table_name, ",
        "t.table_type AS table_type, c.column_name AS column_name, c.data_type AS data_type, ",
        "c.is_nullable AS is_nullable, c.ordinal_position AS ordinal_position ",
        "FROM information_schema.tables AS t LEFT JOIN information_schema.columns AS c ",
        "ON c.table_catalog = t.table_catalog AND c.table_schema = t.table_schema AND c.table_name = t.table_name ",
        "WHERE t.table_type IN ('BASE TABLE', 'VIEW') AND ($catalog_filter IS NULL OR t.table_catalog = $catalog_filter) ",
        "AND ($schema_filter IS NULL OR t.table_schema = $schema_filter) AND ($table_filter IS NULL OR t.table_name = $table_filter) ",
        "ORDER BY t.table_catalog, t.table_schema, t.table_name, c.ordinal_position"
    );
    bind_optional_text(
        sql,
        &[
            ("catalog_filter", scope.table_catalog.as_deref()),
            ("schema_filter", scope.schema_name.as_deref()),
            ("table_filter", scope.table_name.as_deref()),
        ],
    )
}

pub(super) fn node(scope: &MetadataScope) -> ProductJobNodeDefinition {
    ProductJobNodeDefinition {
        id: "collect".into(),
        name: "采集真实表与字段快照".into(),
        definition: json!({"format_version":1,"type":"metadata_information_schema","scope":scope}),
        depends_on: vec![],
    }
}

pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let frozen = &job.definition_snapshot;
    if !job.can_retry
        || job.action != "collect_metadata"
        || job.source_kind != "metadata_collection"
        || frozen["format_version"] != 1
        || frozen["executor"] != "metadata_catalog_v1"
        || frozen["tenant_id"] != job.tenant_id
        || frozen["source_kind"] != job.source_kind
        || frozen["source_id"] != job.source_id
        || frozen["source_version"] != job.source_version
        || job.parameters != json!({})
    {
        return Err(bad("元数据运行身份与固定快照不一致"));
    }
    let scope = scope(&frozen["properties"])?;
    if frozen["scope"] != json!(scope) || job.nodes.len() != 1 {
        return Err(bad("元数据运行范围或节点数无效"));
    }
    let expected = node(&scope);
    let actual = &job.nodes[0];
    if expected.id != actual.id
        || expected.name != actual.name
        || expected.definition != actual.definition
        || expected.depends_on != actual.depends_on
    {
        return Err(bad("采集节点与固定范围不一致"));
    }
    Ok(vec![(expected.id, query(&scope)?)])
}
