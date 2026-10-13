use chrono::{DateTime, Utc};
use dts_backend::{
    ResourceReference,
    repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicDefinition {
    pub process_id: String,
    pub model_id: String,
    pub metadata_object_id: String,
    pub expression: String,
    pub grain: String,
    pub grain_columns: Vec<String>,
    pub time_column: String,
    pub time_timezone: String,
    #[serde(default)]
    pub dimensions: Vec<String>,
    pub unit_id: Option<String>,
    pub unit_version: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedDefinition {
    pub atomic_metric_id: String,
    pub modifier_id: Option<String>,
    pub period_id: String,
    pub dimensions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeDefinition {
    pub metric_ids: Vec<String>,
    pub expression: String,
    pub unit_id: Option<String>,
    pub unit_version: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModifierDefinition {
    pub metadata_object_id: String,
    pub dimension: String,
    pub condition: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodDefinition {
    pub unit: String,
    pub length: u32,
    pub timezone: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDefinition {
    pub symbol: String,
    pub dimension: String,
    #[serde(default = "one")]
    pub conversion_factor: f64,
}
fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenReference {
    pub kind: String,
    pub id: String,
    pub version: i64,
    pub code: String,
    pub status: String,
    pub properties: Value,
    pub references: Vec<ResourceReference>,
    pub publication_hash: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenMetadata {
    pub id: String,
    pub version: i64,
    pub properties: Value,
    pub binding: MetadataEngineBinding,
    pub columns: Vec<MetadataColumn>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricMeasure {
    pub code: String,
    pub aggregate: String,
    pub argument: Option<String>,
    pub modifier: Option<String>,
    pub period: Option<PeriodDefinition>,
    pub unit: Option<UnitDefinition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricPlan {
    pub model_id: String,
    pub model_version: i64,
    pub grain: Vec<String>,
    pub dimensions: Vec<String>,
    pub time_column: String,
    pub time_timezone: String,
    pub measures: Vec<MetricMeasure>,
    pub expression: String,
    pub unit: Option<UnitDefinition>,
    pub dimension: BTreeMap<String, i32>,
    pub numeric_semantics: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricPublication {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub metadata: Option<FrozenMetadata>,
    pub references: Vec<FrozenReference>,
    pub plan: Option<MetricPlan>,
    pub period: Option<PeriodDefinition>,
    pub modifier: Option<ModifierDefinition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub as_of: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub row_limit: u32,
}
fn default_limit() -> u32 {
    1000
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub timezone: String,
    pub source: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSnapshot {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub metadata: FrozenMetadata,
    pub publication: MetricPublication,
    pub request: MetricRunRequest,
    pub window: MetricWindow,
}
