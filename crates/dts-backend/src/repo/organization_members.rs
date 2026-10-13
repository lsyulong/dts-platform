use crate::BackendError;
use async_trait::async_trait;
mod types;
pub mod validation;
pub use types::*;

#[async_trait]
pub trait OrganizationMembersRepo: Send + Sync {
    async fn self_organization(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<Option<String>, BackendError>;
    async fn organization(
        &self,
        params: &ReadOrganizationParams,
    ) -> Result<OrganizationDetails, BackendError>;
    async fn membership(
        &self,
        params: &ReadMembershipParams,
    ) -> Result<UserOrganizationMembership, BackendError>;
    async fn list_members(
        &self,
        filter: &OrganizationMemberFilter,
    ) -> Result<OrganizationMemberPage, BackendError>;
    async fn transfer(
        &self,
        params: &TransferOrganizationMemberParams,
    ) -> Result<OrganizationDecisionResult, BackendError>;
    async fn remove(
        &self,
        params: &RemoveOrganizationMemberParams,
    ) -> Result<OrganizationDecisionResult, BackendError>;
    async fn set_manager(
        &self,
        params: &SetOrganizationManagerParams,
    ) -> Result<OrganizationDecisionResult, BackendError>;
    async fn history(
        &self,
        filter: &OrganizationHistoryFilter,
    ) -> Result<OrganizationHistoryPage, BackendError>;
}
