use crate::ProductResource;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ServiceAuthority {
    pub active: bool,
    pub can_approve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceApplication {
    pub resource: ProductResource,
    pub owner_id: String,
    pub allowed_origins: Vec<String>,
    pub credential_expires_at: DateTime<Utc>,
}

pub struct SaveServiceApplicationParams {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub expected_version: Option<i64>,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub allowed_origins: Vec<String>,
    pub credential_expires_at: DateTime<Utc>,
}
pub struct ArchiveServiceApplicationParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub application_id: String,
    pub expected_version: i64,
}
pub struct ServiceResourceFilter {
    pub tenant_id: String,
    pub actor_id: String,
    /// Only subscription approval lists may use this flag; authority is checked in DB.
    pub approvals: bool,
    pub status: Option<String>,
    pub q: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCredentialSummary {
    pub id: String,
    pub tenant_id: String,
    pub application_id: String,
    pub owner_id: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<String>,
}
pub struct ServiceCredentialRecord {
    pub summary: ServiceCredentialSummary,
    pub verifier: String,
}
impl std::fmt::Debug for ServiceCredentialRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceCredentialRecord")
            .field("summary", &self.summary)
            .field("verifier", &"<REDACTED>")
            .finish()
    }
}
pub struct IssueServiceCredentialParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub application_id: String,
    pub expected_version: i64,
    pub idempotency_key: String,
    pub credential_id: String,
    pub verifier: String,
    /// Caller semantics; a missing value remains missing across network replay.
    pub requested_expires_at: Option<DateTime<Utc>>,
    /// Resolved deadline, ignored when replaying an existing matching receipt.
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceCredentialIssueResult {
    pub application: ProductResource,
    pub credential: ServiceCredentialSummary,
    pub replayed: bool,
}
pub struct RevokeServiceCredentialParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub application_id: String,
    pub credential_id: String,
    pub expected_version: i64,
    pub idempotency_key: String,
}
pub struct ServiceCredentialFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub application_id: String,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Serialize)]
pub struct ServiceCredentialPage {
    pub data: Vec<ServiceCredentialSummary>,
    pub total: i64,
}
