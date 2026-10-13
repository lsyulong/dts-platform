use crate::ProductResource;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct OrganizationScope {
    pub tenant_id: String,
    pub actor_id: String,
}
pub struct ReadOrganizationParams {
    pub scope: OrganizationScope,
    pub organization_id: String,
}
pub struct ReadMembershipParams {
    pub scope: OrganizationScope,
    pub user_id: String,
}
pub struct OrganizationMemberFilter {
    pub scope: OrganizationScope,
    pub organization_id: String,
    pub q: Option<String>,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagerHandover {
    pub expected_organization_version: i64,
    pub successor_user_id: Option<String>,
}
pub struct TransferOrganizationMemberParams {
    pub scope: OrganizationScope,
    pub user_id: String,
    pub expected_membership_version: i64,
    pub target_organization_id: String,
    pub expected_target_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub manager_handover: Option<ManagerHandover>,
}
pub struct RemoveOrganizationMemberParams {
    pub scope: OrganizationScope,
    pub user_id: String,
    pub expected_membership_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub manager_handover: Option<ManagerHandover>,
}
pub struct SetOrganizationManagerParams {
    pub scope: OrganizationScope,
    pub organization_id: String,
    pub expected_organization_version: i64,
    pub manager_user_id: Option<String>,
    pub reason: String,
    pub idempotency_key: String,
}
pub struct OrganizationHistoryFilter {
    pub scope: OrganizationScope,
    pub organization_id: Option<String>,
    pub user_id: Option<String>,
    pub limit: u32,
    pub offset: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationUser {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationMembership {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub organization_id: Option<String>,
    pub version: i64,
    pub changed_by: String,
    pub changed_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOrganizationMembership {
    pub user: OrganizationUser,
    pub membership: Option<OrganizationMembership>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationMember {
    pub user: OrganizationUser,
    pub membership: OrganizationMembership,
    pub is_manager: bool,
}
#[derive(Debug, Serialize)]
pub struct OrganizationMemberPage {
    pub data: Vec<OrganizationMember>,
    pub total: i64,
}
#[derive(Debug, Serialize)]
pub struct OrganizationDetails {
    pub organization: ProductResource,
    pub manager: Option<OrganizationMember>,
    pub manager_valid: bool,
    pub member_count: i64,
    pub children_count: i64,
    pub legacy_membership_ignored: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationIdentity {
    pub id: String,
    pub code: String,
    pub name: String,
    pub version: i64,
    pub parent_id: Option<String>,
}
impl From<&ProductResource> for OrganizationIdentity {
    fn from(r: &ProductResource) -> Self {
        Self {
            id: r.id.clone(),
            code: r.code.clone(),
            name: r.name.clone(),
            version: r.version,
            parent_id: r.parent_id.clone(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationManagerChange {
    pub organization: OrganizationIdentity,
    pub previous_user_id: Option<String>,
    pub manager: Option<OrganizationUser>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationDecisionResult {
    pub membership: Option<OrganizationMembership>,
    pub source: Option<OrganizationIdentity>,
    pub target: Option<OrganizationIdentity>,
    pub manager_changes: Vec<OrganizationManagerChange>,
    pub receipt_id: String,
    pub replayed: bool,
    pub roles_unchanged: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationHistoryEntry {
    pub id: String,
    pub action: String,
    pub actor_id: String,
    pub user_id: Option<String>,
    pub reason: String,
    pub result: OrganizationDecisionResult,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Serialize)]
pub struct OrganizationHistoryPage {
    pub data: Vec<OrganizationHistoryEntry>,
    pub total: i64,
}
