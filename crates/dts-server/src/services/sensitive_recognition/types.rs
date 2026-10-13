// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    ProductResource,
    repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationDefinition {
    pub rank: u32,
    pub handling_policy: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionRule {
    pub classification_id: String,
    pub pattern: String,
    #[serde(default = "sample_value")]
    pub match_target: String,
    #[serde(default = "sample_limit")]
    pub sample_limit: u32,
    #[serde(default = "source_types")]
    pub source_types: Vec<String>,
}
fn sample_value() -> String {
    "sample_value".into()
}
fn sample_limit() -> u32 {
    1000
}
fn source_types() -> Vec<String> {
    vec!["text".into()]
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionTask {
    pub rule_id: String,
    pub object_id: String,
    pub sample_limit: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenResource {
    pub id: String,
    pub version: i64,
    pub properties: Value,
}
impl From<&ProductResource> for FrozenResource {
    fn from(value: &ProductResource) -> Self {
        Self {
            id: value.id.clone(),
            version: value.version,
            properties: value.properties.clone(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionMetadata {
    pub id: String,
    pub version: i64,
    pub properties: Value,
    pub binding: MetadataEngineBinding,
    pub columns: Vec<MetadataColumn>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionSnapshot {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub task: FrozenResource,
    pub rule: FrozenResource,
    pub classification: FrozenResource,
    pub metadata: RecognitionMetadata,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionReviewRequest {
    pub expected_version: i64,
    pub reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionListQuery {
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
