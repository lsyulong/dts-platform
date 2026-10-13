// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, compiler, types::ChartType};
use crate::{models::error::AppError, services::query_svc::QueryResult};
use dts_backend::repo::product_job::ProductJob;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn map_node_result(job: &ProductJob, result: QueryResult) -> Result<Value, AppError> {
    let snapshot = compiler::snapshot(job)?;
    let expected: Vec<_> = snapshot
        .report
        .dimensions
        .iter()
        .chain(snapshot.report.measures.iter().map(|measure| &measure.name))
        .cloned()
        .collect();
    let actual: Vec<_> = result
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    if actual != expected || result.row_count != result.rows.len() {
        return Err(bad("BI 实际查询结果字段与报表定义不一致"));
    }
    if result.rows.len() > snapshot.report.row_limit as usize {
        return Err(bad(
            "BI 聚合分组超过报表行上限，未发布截断结果，请缩小维度范围",
        ));
    }
    let keys: BTreeSet<_> = expected.iter().map(String::as_str).collect();
    if result.rows.iter().any(|row| {
        row.as_object().is_none_or(|row| {
            row.len() != keys.len() || row.keys().any(|name| !keys.contains(name.as_str()))
        })
    }) {
        return Err(bad("BI 查询返回了无效的实际结果行结构"));
    }
    if snapshot.report.chart_config.chart_type != ChartType::Table
        && !result
            .columns
            .iter()
            .any(|column| column.name == snapshot.report.chart_config.y_field && column.is_numeric)
    {
        return Err(bad("图表度量的实际返回类型不是数值，未生成伪造图表"));
    }
    Ok(
        json!({"execution_mode":"read_only","metadata_id":snapshot.metadata.id,"metadata_version":snapshot.metadata.version,
        "dimensions":snapshot.report.dimensions,"measures":snapshot.report.measures,"chart_config":snapshot.report.chart_config,
        "columns":result.columns,"rows":result.rows,"row_count":result.row_count,"suggested_chart":result.suggested_chart,
        "complete":true,"row_limit":snapshot.report.row_limit}),
    )
}
