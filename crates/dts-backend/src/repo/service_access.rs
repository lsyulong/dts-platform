use crate::{BackendError, ProductResource, ResourcePage};
use async_trait::async_trait;

mod application;
mod call;
mod subscription;
pub mod validation;
pub use application::*;
pub use call::*;
pub use subscription::*;

/// Real application identities, subscription decisions, credential receipts,
/// quota reservation and observed call outcomes; all operations are tenant-scoped.
#[async_trait]
pub trait ServiceAccessRepo: Send + Sync {
    async fn actor_authority(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<ServiceAuthority, BackendError>;
    async fn save_application(
        &self,
        params: &SaveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError>;
    async fn get_application(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceApplication>, BackendError>;
    async fn list_applications(
        &self,
        filter: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError>;
    async fn archive_application(
        &self,
        params: &ArchiveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError>;
    async fn issue_credential(
        &self,
        params: &IssueServiceCredentialParams,
    ) -> Result<ServiceCredentialIssueResult, BackendError>;
    async fn revoke_credential(
        &self,
        params: &RevokeServiceCredentialParams,
    ) -> Result<ServiceCredentialSummary, BackendError>;
    async fn list_credentials(
        &self,
        filter: &ServiceCredentialFilter,
    ) -> Result<ServiceCredentialPage, BackendError>;
    /// Internal credential lookup; the verifier is never serialized to an API.
    async fn lookup_credential(
        &self,
        id: &str,
    ) -> Result<Option<ServiceCredentialRecord>, BackendError>;
    async fn save_subscription(
        &self,
        params: &SaveServiceSubscriptionParams,
    ) -> Result<ProductResource, BackendError>;
    async fn get_subscription(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceSubscription>, BackendError>;
    async fn list_subscriptions(
        &self,
        filter: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError>;
    async fn decide_subscription(
        &self,
        params: &DecideServiceSubscriptionParams,
    ) -> Result<ServiceSubscriptionDecisionResult, BackendError>;
    /// This request is produced only after constant-time server credential verification.
    /// The transaction rechecks the immutable verifier/version and current owner,
    /// application, publication, subscription, origin and expiry before reserving.
    async fn reserve_call(
        &self,
        params: &ReserveServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError>;
    async fn reauthorize_call(&self, expected: &ServiceCallRecord) -> Result<(), BackendError>;
    /// Finalization replays identical outcome/row-count semantics while retaining
    /// the first generated duration/timestamp. Failed/denied
    /// finalization remains possible after revocation to preserve truthful audit.
    async fn finish_call(
        &self,
        params: &FinishServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError>;
    async fn list_calls(&self, filter: &ServiceCallFilter)
    -> Result<ServiceCallPage, BackendError>;
}
