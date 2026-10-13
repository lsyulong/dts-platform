use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ProductResource,
    repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataAccessTarget {
    pub metadata_id: String,
    pub binding: MetadataEngineBinding,
    pub columns: Vec<MetadataColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessRequest {
    pub resource: ProductResource,
    pub requester_id: String,
    pub target: DataAccessTarget,
    pub requested_columns: Vec<String>,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
}

pub struct SaveDataAccessRequestParams {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub expected_version: Option<i64>,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub target: DataAccessTarget,
    pub requested_columns: Vec<String>,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataAccessDecision {
    Submit,
    Approve,
    Reject,
    Withdraw,
    Revoke,
}

impl DataAccessDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Withdraw => "withdraw",
            Self::Revoke => "revoke",
        }
    }
    pub fn needs_approver(self) -> bool {
        matches!(self, Self::Approve | Self::Reject | Self::Revoke)
    }
}

pub struct DecideDataAccessParams {
    pub tenant_id: String,
    pub request_id: String,
    pub expected_version: i64,
    pub actor_id: String,
    pub decision: DataAccessDecision,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessGrant {
    pub id: String,
    pub tenant_id: String,
    pub request_id: String,
    pub request_version: i64,
    pub grantee_id: String,
    pub target: DataAccessTarget,
    pub columns: Vec<String>,
    pub granted_by: String,
    pub granted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataAccessDecisionResult {
    pub request: ProductResource,
    pub grant: Option<DataAccessGrant>,
    pub decision_id: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DataAccessAuthority {
    pub active: bool,
    pub can_approve: bool,
    pub all_data: bool,
}

pub struct DataAccessRequestFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub approvals: bool,
    pub status: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessOutcome {
    Succeeded,
    Failed,
    Denied,
}

impl AccessOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataAccessEvidence {
    pub query_hash: String,
    pub grant_ids: Vec<String>,
    pub metadata_ids: Vec<String>,
    pub columns: Vec<String>,
    pub all_data: bool,
}

pub struct RecordDataAccessParams {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub outcome: AccessOutcome,
    pub evidence: DataAccessEvidence,
}

/// Internal fence over published privacy configuration, never an HTTP input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrivacyPolicyVersion {
    pub kind: String,
    pub id: String,
    pub version: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataAccessAudit {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub action: String,
    pub outcome: String,
    pub evidence: Value,
    pub created_at: DateTime<Utc>,
}

pub struct DataAccessAuditFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Serialize)]
pub struct DataAccessAuditPage {
    pub data: Vec<DataAccessAudit>,
    pub total: i64,
}
