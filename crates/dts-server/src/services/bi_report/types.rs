// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregate {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportMeasure {
    pub name: String,
    pub expression: String,
    pub aggregate: Aggregate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartConfig {
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    pub title: String,
    pub x_field: String,
    pub y_field: String,
    pub legend: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportDefinition {
    pub dimensions: Vec<String>,
    pub measures: Vec<ReportMeasure>,
    pub row_limit: u32,
    pub chart_config: ChartConfig,
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
pub struct ReportSnapshot {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub metadata: FrozenMetadata,
    pub report: ReportDefinition,
}
