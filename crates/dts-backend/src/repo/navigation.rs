use crate::BackendError;
use async_trait::async_trait;
mod types;
pub mod validation;
mod view;
pub use types::*;
pub use view::effective_navigation;

#[async_trait]
pub trait NavigationRepo: Send + Sync {
    async fn assign(
        &self,
        params: &AssignNavigationParams,
    ) -> Result<NavigationDecisionResult, BackendError>;
    async fn revoke(
        &self,
        params: &RevokeNavigationParams,
    ) -> Result<NavigationDecisionResult, BackendError>;
    async fn current(&self, params: &ReadNavigationParams) -> Result<NavigationView, BackendError>;
    async fn history(
        &self,
        filter: &NavigationHistoryFilter,
    ) -> Result<NavigationHistoryPage, BackendError>;
}
