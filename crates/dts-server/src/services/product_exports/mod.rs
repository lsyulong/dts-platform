// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product_jobs::{ProductJobService, ProductJobView, ResultAccess},
};
use dts_backend::repo::product_job::{ProductJobNodeStatus, ProductJobStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

mod encode;
#[cfg(test)]
mod tests;
mod types;
pub use types::*;

pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;
const SOURCES: [&str; 4] = [
    "bi_report",
    "sql_document",
    "development_workflow",
    "data_api",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportColumn {
    pub name: String,
    pub data_type: String,
    pub is_numeric: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ExportData {
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub job_id: String,
    pub node_id: String,
    pub columns: Vec<ExportColumn>,
    pub rows: Vec<Value>,
    pub row_count: usize,
}

#[derive(Clone)]
pub struct ResultExportService {
    jobs: Arc<ProductJobService>,
}
impl ResultExportService {
    pub fn new(jobs: Arc<ProductJobService>) -> Self {
        Self { jobs }
    }

    pub async fn export(
        &self,
        actor: &AuthIdentity,
        job_id: &str,
        request: ExportRequest,
    ) -> Result<ExportArtifact, AppError> {
        if request.node_id.is_empty()
            || request.node_id.len() > 128
            || request.expected_source_version <= 0
        {
            return Err(bad("导出节点或期望定义版本无效"));
        }
        // get authorizes the current reader, rechecks current grants and proof,
        // reapplies current masking, validates receipts and records a real audit.
        let current = self.jobs.get(actor, job_id).await?;
        let data = extract(&current, &request.node_id, request.expected_source_version)?;
        let artifact = encode::artifact(&data, request.format)?;
        // Encoding may take time; never release a stale authorization snapshot.
        let latest = self.jobs.get(actor, job_id).await?;
        if extract(&latest, &request.node_id, request.expected_source_version)? != data {
            return Err(AppError::Conflict(
                "导出结果或当前输出策略已变化，请重新获取结果".into(),
            ));
        }
        Ok(artifact)
    }
}

pub(super) fn extract(
    job: &ProductJobView,
    node_id: &str,
    source_version: i64,
) -> Result<ExportData, AppError> {
    if !SOURCES.contains(&job.source_kind.as_str()) || job.status != ProductJobStatus::Succeeded {
        return Err(AppError::Conflict(
            "仅实际成功的明细查询或BI运行可以导出".into(),
        ));
    }
    if job.source_version != source_version {
        return Err(AppError::Conflict("导出期望的定义版本不匹配".into()));
    }
    if job.result_access != ResultAccess::Authorized {
        return Err(AppError::Forbidden(
            "当前读者无权导出已保存结果，请重新授权或运行".into(),
        ));
    }
    let node = job
        .nodes
        .iter()
        .find(|node| node.id == node_id && node.status == ProductJobNodeStatus::Succeeded)
        .ok_or_else(|| AppError::NotFound("真实成功导出节点不存在".into()))?;
    let result = node
        .result
        .as_ref()
        .ok_or_else(|| AppError::Forbidden("当前节点结果受限或不可用".into()))?;
    let columns: Vec<ExportColumn> =
        serde_json::from_value(result["columns"].clone()).map_err(|_| bad("真实结果列结构无效"))?;
    let rows: Vec<Value> =
        serde_json::from_value(result["rows"].clone()).map_err(|_| bad("真实结果行结构无效"))?;
    let row_count = result["row_count"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| bad("真实结果行数无效"))?;
    validate(&columns, &rows, row_count)?;
    Ok(ExportData {
        source_kind: job.source_kind.clone(),
        source_id: job.source_id.clone(),
        source_version: job.source_version,
        job_id: job.id.clone(),
        node_id: node.id.clone(),
        columns,
        rows,
        row_count,
    })
}

pub(super) fn validate(
    columns: &[ExportColumn],
    rows: &[Value],
    row_count: usize,
) -> Result<(), AppError> {
    let names: BTreeSet<_> = columns.iter().map(|column| column.name.as_str()).collect();
    if columns.is_empty()
        || columns.len() > 1000
        || names.len() != columns.len()
        || rows.len() > 10000
        || row_count != rows.len()
        || columns.iter().any(|column| {
            column.name.is_empty() || column.name.len() > 1024 || column.data_type.len() > 1024
        })
    {
        return Err(bad("导出真实字段、行数或上限不符合合同"));
    }
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| bad("导出实际行必须为完整对象"))?;
        if object.len() != names.len() || object.keys().any(|key| !names.contains(key.as_str())) {
            return Err(bad("导出实际行字段缺失或含未声明字段"));
        }
    }
    Ok(())
}
pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
