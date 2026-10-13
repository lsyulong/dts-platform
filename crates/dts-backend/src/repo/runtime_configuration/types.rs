use super::validation;
use crate::{BackendError, ProductResource};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct RuntimeConfigurationScope {
    pub tenant_id: String,
    pub actor_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeConfigurationRange {
    Source,
    Shared,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfigurationPolicy {
    pub scope: RuntimeConfigurationRange,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub variables: Value,
    #[serde(rename = "timeout_seconds")]
    pub query_timeout_seconds: Option<u32>,
    pub concurrency: Option<u32>,
}

/// Frozen control evidence contains no credential or variable values. HTTP run
/// requests never deserialize this type; storage readers validate it again.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfigurationEvidence {
    pub tenant_id: String,
    pub configuration_id: String,
    pub configuration_version: i64,
    pub configuration_hash: String,
    pub query_timeout_seconds: u32,
    pub concurrency: u32,
    pub scope: RuntimeConfigurationRange,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
}

/// A server-only witness minted after validating policy and, for source scope,
/// its real parameter declarations. It cannot be provided by an HTTP body.
#[derive(Clone, Debug)]
pub struct ValidatedRuntimePolicy {
    hash: String,
    source_hash: Option<String>,
}
impl ValidatedRuntimePolicy {
    pub fn from_validated(
        configuration: &ProductResource,
        source: Option<&ProductResource>,
    ) -> Result<Self, BackendError> {
        validation::policy(&configuration.properties)?;
        Ok(Self {
            hash: validation::resource_hash(configuration)?,
            source_hash: source.map(validation::resource_hash).transpose()?,
        })
    }
    pub fn matches(
        &self,
        configuration: &ProductResource,
        source: Option<&ProductResource>,
    ) -> Result<bool, BackendError> {
        Ok(self.hash == validation::resource_hash(configuration)?
            && self.source_hash == source.map(validation::resource_hash).transpose()?)
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedRuntimeSource {
    hash: String,
}
impl ValidatedRuntimeSource {
    pub fn from_validated(source: &ProductResource) -> Result<Self, BackendError> {
        Ok(Self {
            hash: validation::resource_hash(source)?,
        })
    }
    pub fn matches(&self, source: &ProductResource) -> Result<bool, BackendError> {
        Ok(self.hash == validation::resource_hash(source)?)
    }
}

pub struct RuntimeConfigurationLookup {
    pub scope: RuntimeConfigurationScope,
    pub configuration_id: String,
}
pub struct PublishRuntimeConfigurationParams {
    pub lookup: RuntimeConfigurationLookup,
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub validation: Option<ValidatedRuntimePolicy>,
}
pub struct WithdrawRuntimeConfigurationParams {
    pub lookup: RuntimeConfigurationLookup,
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
}
#[derive(Clone, Copy, Debug)]
pub enum RuntimeSourceAccess {
    Read,
    Publish,
    Execute,
}
#[derive(Clone, Debug)]
pub struct RuntimeSourceLookup {
    pub scope: RuntimeConfigurationScope,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub access: RuntimeSourceAccess,
}
/// Internal source input is deliberately not serializable as a public DTO.
pub struct RuntimeSourceInput {
    pub source: ProductResource,
    pub configuration: Option<ProductResource>,
    pub evidence: Option<RuntimeConfigurationEvidence>,
}
pub struct VerifyRuntimeConfigurationParams {
    pub source: RuntimeSourceLookup,
    pub evidence: Option<RuntimeConfigurationEvidence>,
}
pub struct PublishRuntimeSourceParams {
    pub source: RuntimeSourceLookup,
    pub validation: ValidatedRuntimeSource,
    pub evidence: Option<RuntimeConfigurationEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeConfigurationChange {
    pub resource: ProductResource,
    pub evidence: Option<RuntimeConfigurationEvidence>,
    pub receipt_id: String,
    pub replayed: bool,
}
#[derive(Debug, Serialize)]
pub struct RuntimeEvidenceState {
    pub valid: bool,
    pub reason_code: Option<&'static str>,
    pub frozen_evidence_preserved: bool,
}
