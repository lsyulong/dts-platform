use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::repo::organization_members::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferOrganizationMemberRequest {
    pub expected_membership_version: i64,
    pub target_organization_id: String,
    pub expected_target_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub manager_handover: Option<ManagerHandover>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveOrganizationMemberRequest {
    pub expected_membership_version: i64,
    pub reason: String,
    pub idempotency_key: String,
    pub manager_handover: Option<ManagerHandover>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetOrganizationManagerRequest {
    pub expected_organization_version: i64,
    pub manager_user_id: Option<String>,
    pub reason: String,
    pub idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMembersQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationHistoryQuery {
    pub organization_id: Option<String>,
    pub user_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone)]
pub struct OrganizationMembersService {
    repo: Arc<dyn OrganizationMembersRepo>,
}
impl OrganizationMembersService {
    pub fn new(repo: Arc<dyn OrganizationMembersRepo>) -> Self {
        Self { repo }
    }
    pub async fn organization(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<OrganizationDetails, AppError> {
        Ok(self
            .repo
            .organization(&ReadOrganizationParams {
                scope: scope(actor),
                organization_id: id.into(),
            })
            .await?)
    }
    pub async fn membership(
        &self,
        actor: &AuthIdentity,
        user: &str,
    ) -> Result<UserOrganizationMembership, AppError> {
        Ok(self
            .repo
            .membership(&ReadMembershipParams {
                scope: scope(actor),
                user_id: user.into(),
            })
            .await?)
    }
    pub async fn members(
        &self,
        actor: &AuthIdentity,
        id: &str,
        q: OrganizationMembersQuery,
    ) -> Result<OrganizationMemberPage, AppError> {
        Ok(self
            .repo
            .list_members(&OrganizationMemberFilter {
                scope: scope(actor),
                organization_id: id.into(),
                q: q.q,
                limit: q.limit.unwrap_or(20),
                offset: q.offset.unwrap_or(0),
            })
            .await?)
    }
    pub async fn transfer(
        &self,
        actor: &AuthIdentity,
        user: &str,
        r: TransferOrganizationMemberRequest,
    ) -> Result<OrganizationDecisionResult, AppError> {
        Ok(self
            .repo
            .transfer(&TransferOrganizationMemberParams {
                scope: scope(actor),
                user_id: user.into(),
                expected_membership_version: r.expected_membership_version,
                target_organization_id: r.target_organization_id,
                expected_target_version: r.expected_target_version,
                reason: r.reason,
                idempotency_key: r.idempotency_key,
                manager_handover: r.manager_handover,
            })
            .await?)
    }
    pub async fn remove(
        &self,
        actor: &AuthIdentity,
        user: &str,
        r: RemoveOrganizationMemberRequest,
    ) -> Result<OrganizationDecisionResult, AppError> {
        Ok(self
            .repo
            .remove(&RemoveOrganizationMemberParams {
                scope: scope(actor),
                user_id: user.into(),
                expected_membership_version: r.expected_membership_version,
                reason: r.reason,
                idempotency_key: r.idempotency_key,
                manager_handover: r.manager_handover,
            })
            .await?)
    }
    pub async fn manager(
        &self,
        actor: &AuthIdentity,
        id: &str,
        r: SetOrganizationManagerRequest,
    ) -> Result<OrganizationDecisionResult, AppError> {
        Ok(self
            .repo
            .set_manager(&SetOrganizationManagerParams {
                scope: scope(actor),
                organization_id: id.into(),
                expected_organization_version: r.expected_organization_version,
                manager_user_id: r.manager_user_id,
                reason: r.reason,
                idempotency_key: r.idempotency_key,
            })
            .await?)
    }
    pub async fn history(
        &self,
        actor: &AuthIdentity,
        q: OrganizationHistoryQuery,
    ) -> Result<OrganizationHistoryPage, AppError> {
        Ok(self
            .repo
            .history(&OrganizationHistoryFilter {
                scope: scope(actor),
                organization_id: q.organization_id,
                user_id: q.user_id,
                limit: q.limit.unwrap_or(20),
                offset: q.offset.unwrap_or(0),
            })
            .await?)
    }
}
fn scope(actor: &AuthIdentity) -> OrganizationScope {
    OrganizationScope {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
    }
}
