use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualityRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FrozenResource {
    pub id: String,
    pub version: i64,
    pub name: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QualitySnapshot {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub task: FrozenResource,
    pub object: FrozenResource,
    pub metadata: FrozenResource,
    pub rules: Vec<FrozenResource>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FrozenColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QualityNode {
    pub format_version: u32,
    #[serde(rename = "type")]
    pub node_type: String,
    pub rule_index: usize,
    pub rule_id: String,
    pub rule_version: i64,
    pub quality_object_id: String,
    pub metadata_object_id: String,
    pub table_name: String,
    pub columns: Vec<FrozenColumn>,
    pub rule_type: String,
    pub column_name: String,
    pub conditions: Value,
    pub sample_limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct QualityDefinitionValidation {
    pub valid: bool,
    pub rule_count: usize,
    pub table_name: String,
    pub engine_bound: bool,
    pub executed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QualityCheckStatus {
    Passed,
    Failed,
    EmptyData,
    ExecutionError,
    NotCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityCheckResult {
    pub rule_id: String,
    pub rule_version: i64,
    pub quality_object_id: String,
    pub metadata_object_id: String,
    pub checked_rows: u64,
    pub failed_rows: u64,
    pub failure_ratio: Option<f64>,
    pub status: QualityCheckStatus,
    pub evaluation_scope: String,
}

#[derive(Debug, Serialize)]
pub struct QualityResultRecord {
    pub job_id: String,
    pub task_id: String,
    pub task_version: i64,
    pub node_id: String,
    pub rule_id: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: QualityCheckStatus,
    pub checked_rows: Option<u64>,
    pub failed_rows: Option<u64>,
    pub failure_ratio: Option<f64>,
    pub error: Option<String>,
    pub evaluation_scope: String,
}

#[derive(Debug, Serialize)]
pub struct QualityResultPage {
    pub data: Vec<QualityResultRecord>,
    pub total_jobs: i64,
    pub jobs_scanned: usize,
}

#[derive(Debug, Serialize)]
pub struct QualityReport {
    pub task_id: Option<String>,
    pub quality_object_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub completed_runs: u64,
    pub failed_runs: u64,
    pub incomplete_runs: u64,
    pub successful_checks: u64,
    pub empty_checks: u64,
    pub execution_error_checks: u64,
    pub sampled_checks: u64,
    pub checked_rows: u64,
    pub failed_rows: u64,
    pub failure_ratio: Option<f64>,
    pub counting_unit: String,
    pub status: String,
    pub threshold: Option<f64>,
    pub threshold_exceeded: Option<bool>,
    pub window_complete: bool,
    pub jobs_scanned: usize,
    pub total_jobs: i64,
}
