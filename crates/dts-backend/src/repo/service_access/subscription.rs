use crate::ProductResource;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSubscription {
    pub resource: ProductResource,
    pub application_id: String,
    pub owner_id: String,
    pub requester_id: String,
    pub api_id: String,
    pub api_version: i64,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
}
pub struct SaveServiceSubscriptionParams {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub expected_version: Option<i64>,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub application_id: String,
    pub api_id: String,
    pub api_version: i64,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceSubscriptionDecision {
    Submit,
    Approve,
    Reject,
    Revoke,
}
impl ServiceSubscriptionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Revoke => "revoke",
        }
    }
    pub fn needs_approver(self) -> bool {
        matches!(self, Self::Approve | Self::Reject | Self::Revoke)
    }
}
pub struct DecideServiceSubscriptionParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub subscription_id: String,
    pub expected_version: i64,
    pub decision: ServiceSubscriptionDecision,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub idempotency_key: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceSubscriptionDecisionResult {
    pub subscription: ProductResource,
    pub decision_id: String,
    pub replayed: bool,
}
