// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use crate::{BackendError, repo::product_job::ProductJobLease};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitiveColumnEvidence {
    pub column_name: String,
    pub data_type: String,
    pub checked_values: u32,
    pub matched_values: u32,
    pub null_values: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitiveScanResult {
    pub evaluation_scope: String,
    pub sample_limit: u32,
    pub sampled_rows: u32,
    pub columns: Vec<SensitiveColumnEvidence>,
    pub definition_hash: String,
    pub producer_committed: bool,
    pub finding_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensitiveFinding {
    pub id: String,
    pub tenant_id: String,
    pub task_id: String,
    pub task_version: i64,
    pub job_id: String,
    pub node_id: String,
    pub object_id: String,
    pub object_version: i64,
    pub rule_id: String,
    pub rule_version: i64,
    pub classification_id: String,
    pub classification_version: i64,
    pub column_name: String,
    pub evidence: SensitiveColumnEvidence,
    pub evaluation_scope: String,
    pub sampled_rows: u32,
    pub definition_hash: String,
    pub status: String,
    pub version: i64,
    pub reviewed_by: Option<String>,
    pub review_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}
pub struct SensitiveCommitParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub job_id: String,
    pub node_id: String,
    pub lease: ProductJobLease,
    pub snapshot: Value,
    pub result: SensitiveScanResult,
}
pub struct SensitiveReviewParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub finding_id: String,
    pub expected_version: i64,
    pub confirm: bool,
    pub reason: String,
}
pub struct SensitiveFindingFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Serialize)]
pub struct SensitiveFindingPage {
    pub data: Vec<SensitiveFinding>,
    pub total: i64,
}
#[async_trait]
pub trait SensitiveFindingRepo: Send + Sync {
    /// Commits the actual single-node run, findings and resource projections in
    /// one transaction under a live fenced lease. No client-facing producer.
    async fn commit_scan(
        &self,
        params: &SensitiveCommitParams,
    ) -> Result<SensitiveScanResult, BackendError>;
    async fn review(
        &self,
        params: &SensitiveReviewParams,
    ) -> Result<SensitiveFinding, BackendError>;
    async fn list(
        &self,
        filter: &SensitiveFindingFilter,
    ) -> Result<SensitiveFindingPage, BackendError>;
    async fn get(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<SensitiveFinding>, BackendError>;
}
pub fn snapshot_hash(value: &Value) -> Result<String, BackendError> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid("敏感识别快照无法编码"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_result(snapshot: &Value, result: &SensitiveScanResult) -> Result<(), BackendError> {
    let mode = snapshot["rule"]["properties"]["match_target"]
        .as_str()
        .unwrap_or("sample_value");
    let source_types = snapshot["rule"]["properties"]["source_types"].as_array();
    let source = snapshot["metadata"]["columns"]
        .as_array()
        .ok_or_else(|| invalid("实际字段快照无效"))?;
    let expected: std::collections::BTreeMap<_, _> = source
        .iter()
        .filter(|c| {
            recognition_source_family(c["data_type"].as_str().unwrap_or("")).is_some_and(|family| {
                mode == "column_name"
                    || source_types.map_or(family == "text", |types| {
                        types.iter().any(|v| v.as_str() == Some(family))
                    })
            })
        })
        .map(|c| {
            (
                c["name"].as_str().unwrap_or(""),
                c["data_type"].as_str().unwrap_or(""),
            )
        })
        .collect();
    if result.definition_hash != snapshot_hash(snapshot)?
        || result.producer_committed
        || !result.finding_ids.is_empty()
        || !(1..=1000).contains(&result.sample_limit)
        || snapshot["properties"]["sample_limit"].as_u64() != Some(u64::from(result.sample_limit))
        || result.sampled_rows > result.sample_limit
        || result.columns.is_empty()
        || result.columns.len() > 128
        || !matches!(mode, "sample_value" | "column_name")
        || result.columns.len() != expected.len()
        || result.evaluation_scope
            != if mode == "column_name" {
                "column_names"
            } else {
                "sample"
            }
        || (mode == "column_name" && result.sampled_rows != 0)
    {
        return Err(invalid("敏感识别结果、范围或真实定义摘要无效"));
    }
    let mut names = std::collections::BTreeSet::new();
    for column in &result.columns {
        let max = if mode == "column_name" {
            1
        } else {
            result.sampled_rows
        };
        if column.column_name.is_empty()
            || column.column_name.len() > 1024
            || !names.insert(&column.column_name)
            || expected.get(column.column_name.as_str()) != Some(&column.data_type.as_str())
            || column.matched_values > column.checked_values
            || column.checked_values > max
            || column.null_values > max
            || column.checked_values.checked_add(column.null_values) != Some(max)
        {
            return Err(invalid("敏感识别列计数不完整或重复"));
        }
    }
    Ok(())
}
pub fn recognition_source_family(data_type: &str) -> Option<&'static str> {
    let dtype = data_type.to_ascii_uppercase().replace(' ', "");
    if dtype.starts_with("VARCHAR")
        || dtype.starts_with("CHAR")
        || matches!(dtype.as_str(), "TEXT" | "STRING")
    {
        Some("text")
    } else if matches!(
        dtype.as_str(),
        "TINYINT"
            | "SMALLINT"
            | "INTEGER"
            | "INT"
            | "BIGINT"
            | "UTINYINT"
            | "USMALLINT"
            | "UINTEGER"
            | "UBIGINT"
            | "FLOAT"
            | "REAL"
            | "DOUBLE"
    ) || dtype.starts_with("DECIMAL(")
    {
        Some("number")
    } else if matches!(
        dtype.as_str(),
        "DATE" | "TIMESTAMP" | "TIMESTAMP_S" | "TIMESTAMP_MS" | "TIMESTAMP_NS"
    ) {
        Some("date")
    } else if matches!(dtype.as_str(), "BOOLEAN" | "BOOL") {
        Some("boolean")
    } else {
        None
    }
}
pub fn validate_review(params: &SensitiveReviewParams) -> Result<(), BackendError> {
    if params.expected_version < 1
        || params.reason.trim().is_empty()
        || params.reason.len() > 2000
        || params
            .reason
            .chars()
            .any(|c| c.is_control() && !['\r', '\n', '\t'].contains(&c))
    {
        return Err(invalid("复核版本和必填意见必须有效且最多2000字节"));
    }
    Ok(())
}
fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
