// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    bad, compiler, schema,
    types::{ApiPublication, ApiResponseField},
};
use crate::{models::error::AppError, services::query_svc::QueryResult};
use dts_backend::repo::product_job::ProductJob;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn map_node_result(job: &ProductJob, result: QueryResult) -> Result<Value, AppError> {
    let snapshot = compiler::snapshot(job)?;
    let mut value = map_api_result(&snapshot.publication, result)?;
    value["invocation_scope"] = json!("authenticated_console_jobs");
    value["external_credentials_ready"] = json!(false);
    value["rate_limit_enforced"] = json!(false);
    Ok(value)
}

pub(super) fn map_api_result(
    publication: &ApiPublication,
    result: QueryResult,
) -> Result<Value, AppError> {
    let api = &publication.api;
    validate_result(&api.response_schema, api.row_limit, &result)?;
    Ok(map_value(publication, result))
}

pub(super) fn validate_result(
    response_schema: &[ApiResponseField],
    row_limit: u32,
    result: &QueryResult,
) -> Result<(), AppError> {
    if result.row_count != result.rows.len()
        || result.rows.len() > row_limit as usize
        || result.columns.len() != response_schema.len()
    {
        return Err(bad(
            "API 返回量超过限制或字段数量不匹配，未返回截断的成功数据",
        ));
    }
    for (column, field) in result.columns.iter().zip(response_schema) {
        if column.name != field.name || !schema::actual_type(&field.data_type, &column.data_type) {
            return Err(bad("API 实际返回字段或类型不符合本阶段的真实响应 schema"));
        }
    }
    let names: BTreeSet<_> = response_schema
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    for row in &result.rows {
        let row = row
            .as_object()
            .ok_or_else(|| bad("API 实际返回行不是对象"))?;
        if row.len() != names.len()
            || row.keys().any(|name| !names.contains(name.as_str()))
            || response_schema.iter().any(|field| {
                row.get(&field.name)
                    .is_none_or(|value| !schema::actual_value(field, value))
            })
        {
            return Err(bad("API 实际返回值、空值或字段集合与本阶段 schema 不一致"));
        }
    }
    Ok(())
}

pub(super) fn map_value(publication: &ApiPublication, result: QueryResult) -> Value {
    let api = &publication.api;
    json!({"execution_mode":"read_only",
        "api_id":publication.source_id,"api_version":publication.source_version,"metadata_id":publication.metadata.id,"metadata_version":publication.metadata.version,
        "response_schema":api.response_schema,"row_limit":api.row_limit,"columns":result.columns,"rows":result.rows,
        "row_count":result.row_count,"suggested_chart":result.suggested_chart,"complete":true})
}
