use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{BackendError, ProductResource, ResourcePage};

mod types;
pub mod validation;
pub use types::*;

/// Decisions, source resource state, grants and audit receipts commit together.
#[async_trait]
pub trait DataAccessRepo: Send + Sync {
    async fn actor_authority(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<DataAccessAuthority, BackendError>;
    async fn save_request(
        &self,
        params: &SaveDataAccessRequestParams,
    ) -> Result<ProductResource, BackendError>;
    async fn get_request(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Option<DataAccessRequest>, BackendError>;
    async fn decide(
        &self,
        params: &DecideDataAccessParams,
    ) -> Result<DataAccessDecisionResult, BackendError>;
    async fn list_requests(
        &self,
        filter: &DataAccessRequestFilter,
    ) -> Result<ResourcePage, BackendError>;
    async fn active_grants(
        &self,
        tenant_id: &str,
        actor_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<DataAccessGrant>, BackendError>;
    async fn record_access(&self, params: &RecordDataAccessParams) -> Result<(), BackendError>;
    async fn list_audit(
        &self,
        filter: &DataAccessAuditFilter,
    ) -> Result<DataAccessAuditPage, BackendError>;
}
