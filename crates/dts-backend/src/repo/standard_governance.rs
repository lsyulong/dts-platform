use crate::{BackendError, ProductResource};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const STANDARD_KINDS: [&str; 4] = [
    "naming_standard",
    "field_standard",
    "enum_standard",
    "unit_standard",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenStandardRef {
    pub kind: String,
    pub id: String,
    pub revision_version: i64,
    pub definition_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardRevision {
    pub tenant_id: String,
    pub kind: String,
    pub id: String,
    pub revision_version: i64,
    pub code: String,
    pub name: String,
    pub properties: Value,
    pub children: Vec<FrozenStandardRef>,
    pub definition_hash: String,
    pub state: String,
    pub state_version: i64,
    pub published_by: String,
    pub published_at: DateTime<Utc>,
    pub withdrawn_by: Option<String>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub withdrawal_reason: Option<String>,
}
impl StandardRevision {
    pub fn reference(&self) -> FrozenStandardRef {
        FrozenStandardRef {
            kind: self.kind.clone(),
            id: self.id.clone(),
            revision_version: self.revision_version,
            definition_hash: self.definition_hash.clone(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct StandardScope {
    pub tenant_id: String,
    pub actor_id: String,
    pub kind: String,
    pub id: String,
    pub expected_version: i64,
}
#[derive(Debug, Clone)]
pub enum StandardCommand {
    Publish {
        expected_properties: Value,
        normalized_properties: Value,
        children: Vec<FrozenStandardRef>,
    },
    Revise,
    Withdraw {
        revision_version: i64,
        expected_state_version: i64,
        reason: String,
    },
}
pub struct StandardTransitionParams {
    pub scope: StandardScope,
    pub command: StandardCommand,
}
#[derive(Debug, Serialize)]
pub struct StandardTransitionResult {
    pub resource: ProductResource,
    pub revision: Option<StandardRevision>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardCoverage {
    BoundFields,
    AllFields,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelColumnStandardBinding {
    pub column_name: String,
    pub field_standard: Option<FrozenStandardRef>,
    pub naming_standard: Option<FrozenStandardRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStandardBindingSet {
    pub tenant_id: String,
    pub model_id: String,
    pub model_version: i64,
    pub binding_version: i64,
    pub coverage: StandardCoverage,
    pub columns: Vec<ModelColumnStandardBinding>,
    pub table_naming: Option<FrozenStandardRef>,
    pub model_properties_hash: String,
    pub definition_hash: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}
pub struct BindModelStandardsParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub model_id: String,
    pub expected_model_version: i64,
    pub expected_binding_version: Option<i64>,
    pub expected_properties: Value,
    pub properties: Value,
    pub coverage: StandardCoverage,
    pub columns: Vec<ModelColumnStandardBinding>,
    pub table_naming: Option<FrozenStandardRef>,
    /// Explicitly places a published model back into draft for actual physical reconciliation.
    pub upgrade: bool,
}
#[derive(Debug, Serialize)]
pub struct BindModelStandardsResult {
    pub resource: ProductResource,
    pub bindings: ModelStandardBindingSet,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStandardsState {
    NotConfigured,
    Validated,
    Invalidated,
}
pub struct BeginStandardModelPublicationParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub model_id: String,
    pub expected_model_version: i64,
    pub expected_properties: Value,
    pub binding_version: Option<i64>,
    pub binding_hash: Option<String>,
    pub dependencies: Vec<FrozenStandardRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardModelPublicationClaimRecord {
    pub gate_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub claim_version: i64,
    pub resource: ProductResource,
    pub bindings: Option<ModelStandardBindingSet>,
    pub dependencies: Vec<FrozenStandardRef>,
    pub standards_state: ModelStandardsState,
    pub needs_reconciliation: bool,
}
#[derive(Debug, Clone)]
pub enum StandardModelEngineOutcome {
    Published {
        receipt: Value,
    },
    Failed {
        reason: String,
    },
    Unknown {
        reason: String,
        receipt: Option<Value>,
    },
}
pub struct FinishStandardModelPublicationParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub gate_id: String,
    pub expected_claim_version: i64,
    pub outcome: StandardModelEngineOutcome,
}
#[derive(Debug, Serialize)]
pub struct StandardModelPublicationResult {
    pub resource: ProductResource,
    pub publication: ProductResource,
    pub standards_state: ModelStandardsState,
}
pub struct StandardRevisionFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub kind: String,
    pub id: String,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Serialize)]
pub struct StandardRevisionPage {
    pub data: Vec<StandardRevision>,
    pub total: i64,
}
pub struct StandardConsumerLookup {
    pub tenant_id: String,
    pub actor_id: String,
    pub model_id: String,
}
#[derive(Debug, Clone)]
pub struct StandardInspectionParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub model_id: String,
    pub expected_model_version: i64,
    pub expected_binding_version: i64,
    pub metadata_id: String,
    pub metadata_version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardInspectionSnapshot {
    pub resource: ProductResource,
    pub bindings: ModelStandardBindingSet,
    pub revisions: Vec<StandardRevision>,
    pub metadata: ProductResource,
    pub binding: crate::repo::metadata_catalog::MetadataEngineBinding,
    pub metadata_observed_at: DateTime<Utc>,
    pub publication: ProductResource,
    pub snapshot_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardInspectionIssue {
    pub column_name: Option<String>,
    pub code: String,
    pub message: String,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardInspectionResult {
    pub id: String,
    pub model_id: String,
    pub model_version: i64,
    pub binding_version: i64,
    pub metadata_id: String,
    pub metadata_version: i64,
    pub metadata_observed_at: DateTime<Utc>,
    pub source_fingerprint: String,
    pub evaluation_scope: String,
    pub state: String,
    pub issues: Vec<StandardInspectionIssue>,
    pub data_values_evaluated: bool,
    pub unit_semantics_evaluated: bool,
    pub dependency_versions: Vec<FrozenStandardRef>,
    pub created_at: DateTime<Utc>,
}
pub struct RecordStandardInspectionParams {
    pub source: StandardInspectionParams,
    pub snapshot_hash: String,
    pub result: StandardInspectionResult,
}

pub fn standard_hash(value: &Value) -> Result<String, BackendError> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| BackendError::Internal("标准证据无法编码".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn revision_hash(
    kind: &str,
    code: &str,
    name: &str,
    properties: &Value,
    children: &[FrozenStandardRef],
) -> Result<String, BackendError> {
    standard_hash(
        &serde_json::json!({"kind":kind,"code":code,"name":name,"properties":properties,"children":children}),
    )
}
pub fn binding_hash(
    coverage: StandardCoverage,
    columns: &[ModelColumnStandardBinding],
    table_naming: &Option<FrozenStandardRef>,
    model_properties_hash: &str,
) -> Result<String, BackendError> {
    standard_hash(
        &serde_json::json!({"coverage":coverage,"columns":columns,"table_naming":table_naming,"model_properties_hash":model_properties_hash}),
    )
}

#[async_trait]
pub trait StandardGovernanceRepo: Send + Sync {
    async fn transition(
        &self,
        p: &StandardTransitionParams,
    ) -> Result<StandardTransitionResult, BackendError>;
    async fn revision(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Option<StandardRevision>, BackendError>;
    async fn revisions(
        &self,
        p: &StandardRevisionFilter,
    ) -> Result<StandardRevisionPage, BackendError>;
    async fn bindings(
        &self,
        p: &StandardConsumerLookup,
    ) -> Result<Option<ModelStandardBindingSet>, BackendError>;
    async fn bind_model(
        &self,
        p: &BindModelStandardsParams,
    ) -> Result<BindModelStandardsResult, BackendError>;
    async fn begin_model_publication(
        &self,
        p: &BeginStandardModelPublicationParams,
    ) -> Result<StandardModelPublicationClaimRecord, BackendError>;
    async fn finish_model_publication(
        &self,
        p: &FinishStandardModelPublicationParams,
    ) -> Result<StandardModelPublicationResult, BackendError>;
    async fn inspection_snapshot(
        &self,
        p: &StandardInspectionParams,
    ) -> Result<StandardInspectionSnapshot, BackendError>;
    async fn record_inspection(
        &self,
        p: &RecordStandardInspectionParams,
    ) -> Result<StandardInspectionResult, BackendError>;
}
