use super::validation;
use crate::{BackendError, ProductResource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct DevelopmentRevisionScope {
    pub tenant_id: String,
    pub actor_id: String,
    pub kind: String,
    pub resource_id: String,
}

/// Server-only witness. Callers must validate the current catalog, read-only AST,
/// typed parameters and DAG before minting; no HTTP DTO can deserialize it.
#[derive(Debug, Clone)]
pub struct ValidatedDevelopmentDefinition {
    hash: String,
}
impl ValidatedDevelopmentDefinition {
    pub fn from_validated_snapshot(snapshot: &ProductResource) -> Result<Self, BackendError> {
        Ok(Self {
            hash: validation::snapshot_hash(snapshot)?,
        })
    }
    pub fn matches(&self, snapshot: &ProductResource) -> Result<bool, BackendError> {
        Ok(self.hash == validation::snapshot_hash(snapshot)?)
    }
}

pub struct SaveDevelopmentRevisionParams {
    pub scope: DevelopmentRevisionScope,
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    /// None permits an original receipt replay after the source has changed.
    pub validation: Option<ValidatedDevelopmentDefinition>,
}
pub struct RestoreDevelopmentRevisionParams {
    pub scope: DevelopmentRevisionScope,
    pub revision_id: String,
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub validation: ValidatedDevelopmentDefinition,
}
pub struct ReadDevelopmentRevisionParams {
    pub scope: DevelopmentRevisionScope,
    pub revision_id: String,
}
pub struct DevelopmentRevisionFilter {
    pub scope: DevelopmentRevisionScope,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopmentRevisionSummary {
    pub id: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub snapshot_hash: String,
    pub saved_by: String,
    pub saved_at: DateTime<Utc>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopmentRevision {
    #[serde(flatten)]
    pub summary: DevelopmentRevisionSummary,
    pub snapshot: ProductResource,
}
#[derive(Debug, Serialize)]
pub struct DevelopmentRevisionPage {
    pub data: Vec<DevelopmentRevisionSummary>,
    pub total: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveDevelopmentRevisionResult {
    pub revision: DevelopmentRevisionSummary,
    pub receipt_id: String,
    pub created: bool,
    pub replayed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreDevelopmentRevisionResult {
    pub resource: ProductResource,
    pub revision: DevelopmentRevisionSummary,
    pub receipt_id: String,
    pub replayed: bool,
    pub requires_publish: bool,
}
