use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct ReserveServiceCallParams {
    pub call_id: String,
    pub tenant_id: String,
    pub credential_id: String,
    pub credential_version: i64,
    /// Internal immutable verifier read and checked in constant time by the service.
    pub credential_verifier: String,
    pub api_id: String,
    pub origin: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCallOutcome {
    Pending,
    Succeeded,
    Failed,
    Denied,
    Cancelled,
    ReconciliationRequired,
}
impl ServiceCallOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::ReconciliationRequired => "reconciliation_required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceCallRecord {
    pub id: String,
    pub tenant_id: String,
    pub owner_id: String,
    pub application_id: String,
    pub application_version: i64,
    pub credential_id: String,
    pub credential_version: i64,
    pub subscription_id: String,
    pub subscription_version: i64,
    pub api_id: String,
    pub api_version: i64,
    pub minute_bucket: DateTime<Utc>,
    pub rate_limit: u32,
    pub origin: Option<String>,
    pub outcome: ServiceCallOutcome,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub row_count: Option<u64>,
}
pub struct FinishServiceCallParams {
    pub call: ServiceCallRecord,
    pub outcome: ServiceCallOutcome,
    pub duration_ms: u64,
    pub row_count: Option<u64>,
    /// Internal authorized query evidence committed with the actual call result.
    /// This input is never accepted from an HTTP request.
    pub access_audit: Option<crate::repo::data_access::RecordDataAccessParams>,
    pub masking_versions: Option<Vec<crate::repo::data_access::PrivacyPolicyVersion>>,
}
pub struct ServiceCallFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub application_id: Option<String>,
    pub api_id: Option<String>,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Serialize)]
pub struct ServiceCallPage {
    pub data: Vec<ServiceCallAudit>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallAudit {
    pub id: String,
    pub tenant_id: String,
    pub owner_id: String,
    pub application_id: String,
    pub credential_id: String,
    pub api_id: Option<String>,
    pub target_hash: String,
    pub outcome: ServiceCallOutcome,
    pub reason_code: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub row_count: Option<u64>,
}
