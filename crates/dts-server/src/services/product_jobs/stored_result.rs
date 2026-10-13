// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::repo::product_job::ProductJob;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    models::error::AppError,
    services::{
        data_masking::{MaskingPlan, MaskingReceipt},
        query_svc::{ChartSuggestion, ColumnMeta, QueryResult},
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedColumn {
    name: String,
    data_type: String,
    is_numeric: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedChart {
    chart_type: String,
    x_column: Option<String>,
    y_column: Option<String>,
    reason: String,
}

pub(super) fn decode(value: &Value) -> Result<(QueryResult, Option<MaskingReceipt>), AppError> {
    let invalid = || AppError::Conflict("历史查询结果结构不完整".into());
    let columns: Vec<SavedColumn> =
        serde_json::from_value(value["columns"].clone()).map_err(|_| invalid())?;
    let rows: Vec<Value> = serde_json::from_value(value["rows"].clone()).map_err(|_| invalid())?;
    let row_count = value["row_count"]
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(invalid)?;
    if columns.len() > 1000 || rows.len() > 10000 || row_count != rows.len() {
        return Err(invalid());
    }
    let suggested_chart = value
        .get("suggested_chart")
        .filter(|chart| !chart.is_null())
        .map(|chart| serde_json::from_value::<SavedChart>(chart.clone()).map_err(|_| invalid()))
        .transpose()?
        .map(|chart| ChartSuggestion {
            chart_type: chart.chart_type,
            x_column: chart.x_column,
            y_column: chart.y_column,
            reason: chart.reason,
        });
    let receipt = value
        .get("privacy")
        .map(|receipt| serde_json::from_value(receipt.clone()).map_err(|_| invalid()))
        .transpose()?;
    Ok((
        QueryResult {
            columns: columns
                .into_iter()
                .map(|column| ColumnMeta {
                    name: column.name,
                    data_type: column.data_type,
                    is_numeric: column.is_numeric,
                })
                .collect(),
            rows,
            row_count,
            suggested_chart,
        },
        receipt,
    ))
}

pub(super) fn project(
    job: &ProductJob,
    result: QueryResult,
    receipt: MaskingReceipt,
    plan: &MaskingPlan,
) -> Result<Value, AppError> {
    let mut value = if job.source_kind == "bi_report" {
        crate::services::bi_report::map_node_result(job, result)?
    } else if job.source_kind == "data_api" {
        crate::services::data_services::map_stored_node_result(job, result, plan)?
    } else if crate::services::metrics::is_calculable(&job.source_kind) {
        crate::services::metrics::map_node_result(job, result)?
    } else {
        serde_json::to_value(result)
            .map_err(|_| AppError::Internal("历史查询结果无法编码".into()))?
    };
    value["privacy"] = json!(receipt);
    Ok(value)
}
