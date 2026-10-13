use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    BackendError, ProductResource, ProductResourceFilter, ResourceReference, ResourceWrite,
};

pub mod validation;

pub const ASSET_SOURCE_KINDS: &[&str] = &[
    "metadata_object",
    "data_model",
    "atomic_metric",
    "derived_metric",
    "composite_metric",
    "field_standard",
    "enum_standard",
    "unit_standard",
];

pub struct SaveAssetParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub id: String,
    pub kind: String,
    pub expected_version: Option<i64>,
    pub fields: ResourceWrite,
}

pub struct AssetScope {
    pub tenant_id: String,
    pub actor_id: String,
    pub id: String,
    pub expected_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetTransition {
    Register,
    Publish,
    Withdraw,
}

pub struct TransitionAssetParams {
    pub scope: AssetScope,
    pub action: AssetTransition,
    pub validated_source: Option<AssetSourceCheck>,
}

pub struct AssetSourceCheck {
    pub kind: String,
    pub id: String,
    pub version: i64,
    pub definition_hash: String,
}

pub struct AssetReadParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetPublication {
    pub asset_version: i64,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub definition_hash: String,
    pub proof: Value,
    pub published_by: String,
    pub published_at: DateTime<Utc>,
    pub publication_scope: String,
    pub physical_published: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetView {
    pub resource: ProductResource,
    pub publication: Option<AssetPublication>,
    pub source_valid: bool,
    pub stale: bool,
    pub invalid_reason: Option<String>,
    pub current_source_version: Option<i64>,
}

pub struct AssetListParams {
    pub actor_id: String,
    pub filter: ProductResourceFilter,
}
#[derive(Debug, Serialize)]
pub struct AssetPage {
    pub data: Vec<AssetView>,
    pub total: i64,
}

pub struct AssetMapSnapshotParams {
    pub scope: AssetScope,
    pub max_nodes: u32,
    pub max_edges: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssetGraphNode {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
    pub version: i64,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub publication_version: Option<i64>,
    pub stale: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssetGraphEdge {
    pub source_asset_id: String,
    pub target_asset_id: String,
    pub origin: String,
    pub evidence: Value,
}
#[derive(Debug, Clone, Serialize)]
pub struct InvalidAssetNode {
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct AssetMapSnapshot {
    pub nodes: Vec<AssetGraphNode>,
    pub edges: Vec<AssetGraphEdge>,
    pub invalid_nodes: Vec<InvalidAssetNode>,
    pub truncated: bool,
    pub truncation_reasons: Vec<String>,
    pub can_read_metadata: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetUsageRequest {
    pub asset_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
pub struct AssetUsageParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub request: AssetUsageRequest,
}
#[derive(Debug, Serialize)]
pub struct AssetUsage {
    pub asset_id: String,
    pub metadata_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub instrumentation: String,
    pub scope: String,
    pub complete: bool,
    pub limit: u32,
    pub matched_recorded_events: Option<i64>,
    pub counted_recorded_events: u32,
    pub succeeded: Option<u32>,
    pub failed: Option<u32>,
    pub denied: Option<u32>,
    pub unique_actors: Option<u32>,
}

#[async_trait]
pub trait AssetsRepo: Send + Sync {
    async fn save(&self, params: &SaveAssetParams) -> Result<ProductResource, BackendError>;
    async fn transition(&self, params: &TransitionAssetParams) -> Result<AssetView, BackendError>;
    async fn delete_draft(&self, kind: &str, scope: &AssetScope) -> Result<(), BackendError>;
    async fn get_asset(&self, params: &AssetReadParams) -> Result<Option<AssetView>, BackendError>;
    async fn list_assets(&self, params: &AssetListParams) -> Result<AssetPage, BackendError>;
    async fn map_snapshot(
        &self,
        params: &AssetMapSnapshotParams,
    ) -> Result<AssetMapSnapshot, BackendError>;
    async fn analyze_usage(&self, params: &AssetUsageParams) -> Result<AssetUsage, BackendError>;
}

pub fn source_permission(kind: &str) -> Option<&'static str> {
    match kind {
        "metadata_object" => Some("metadata:read"),
        "data_model" => Some("models:read"),
        "atomic_metric" | "derived_metric" | "composite_metric" => Some("metrics:read"),
        "field_standard" | "enum_standard" | "unit_standard" => Some("standards:read"),
        _ => None,
    }
}

pub fn reference(field: &str, kind: &str, id: &str) -> ResourceReference {
    ResourceReference {
        field: field.into(),
        target_kind: kind.into(),
        target_id: id.into(),
    }
}
