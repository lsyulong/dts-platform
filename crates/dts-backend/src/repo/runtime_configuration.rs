use crate::{BackendError, ProductResource};
use async_trait::async_trait;
mod types;
pub use types::*;
pub mod validation;

#[async_trait]
pub trait RuntimeConfigurationRepo: Send + Sync {
    async fn get(
        &self,
        params: &RuntimeConfigurationLookup,
    ) -> Result<ProductResource, BackendError>;
    async fn publish(
        &self,
        params: &PublishRuntimeConfigurationParams,
    ) -> Result<RuntimeConfigurationChange, BackendError>;
    async fn withdraw(
        &self,
        params: &WithdrawRuntimeConfigurationParams,
    ) -> Result<RuntimeConfigurationChange, BackendError>;
    async fn current_source(
        &self,
        params: &RuntimeSourceLookup,
    ) -> Result<RuntimeSourceInput, BackendError>;
    async fn verify(
        &self,
        params: &VerifyRuntimeConfigurationParams,
    ) -> Result<RuntimeEvidenceState, BackendError>;
    async fn publish_source(
        &self,
        params: &PublishRuntimeSourceParams,
    ) -> Result<ProductResource, BackendError>;
}
