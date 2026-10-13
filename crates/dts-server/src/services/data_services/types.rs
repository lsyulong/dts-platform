// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    ProductResource,
    repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataApiRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
    #[serde(default = "empty_parameters")]
    pub parameters: Value,
}
fn empty_parameters() -> Value {
    json!({})
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiResponseField {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiMetadata {
    pub id: String,
    pub version: i64,
    pub properties: Value,
    pub binding: MetadataEngineBinding,
    pub columns: Vec<MetadataColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiDefinition {
    pub source_sql: String,
    pub compiled_sql: String,
    pub parameters: Value,
    pub response_schema: Vec<ApiResponseField>,
    pub row_limit: u32,
    pub rate_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiPublication {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_id: String,
    pub source_version: i64,
    pub published_by: String,
    pub published_at: String,
    pub metadata: ApiMetadata,
    pub api: ApiDefinition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiJobSnapshot {
    pub format_version: u32,
    pub executor: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub properties: Value,
    pub metadata: ApiMetadata,
    pub publication: ApiPublication,
}

/// Only DataApiService can mint this call after resolving the current published
/// definition and internal metadata proof. It contains no application secret.
pub struct PreparedDataApiCall {
    pub(super) actor_id: String,
    pub(super) tenant_id: String,
    pub(super) source: ProductResource,
    pub(super) publication: ApiPublication,
    pub(super) sql: String,
}

/// Source-validated output after the opaque current masking policy is applied.
pub struct ProtectedDataApiResult {
    pub(super) result: crate::services::query_svc::QueryResult,
    pub(super) source_id: String,
    pub(super) source_version: i64,
    pub(super) schema: Vec<ApiResponseField>,
    pub(super) policy_hash: String,
    pub(super) mask_applied: bool,
    pub(super) invocation_identity: Option<(String, String, String)>,
}
impl ProtectedDataApiResult {
    pub fn query_result(&self) -> &crate::services::query_svc::QueryResult {
        &self.result
    }
}

impl PreparedDataApiCall {
    pub fn sql(&self) -> &str {
        &self.sql
    }
}
