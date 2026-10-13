use crate::{BackendError, ProductResource};
use async_trait::async_trait;

mod types;
pub mod validation;
pub use types::*;

/// Immutable definition archives. These records neither publish nor execute SQL.
#[async_trait]
pub trait DevelopmentRevisionRepo: Send + Sync {
    async fn current(
        &self,
        scope: &DevelopmentRevisionScope,
    ) -> Result<ProductResource, BackendError>;
    async fn save(
        &self,
        params: &SaveDevelopmentRevisionParams,
    ) -> Result<SaveDevelopmentRevisionResult, BackendError>;
    async fn history(
        &self,
        filter: &DevelopmentRevisionFilter,
    ) -> Result<DevelopmentRevisionPage, BackendError>;
    async fn detail(
        &self,
        params: &ReadDevelopmentRevisionParams,
    ) -> Result<DevelopmentRevision, BackendError>;
    async fn restore(
        &self,
        params: &RestoreDevelopmentRevisionParams,
    ) -> Result<RestoreDevelopmentRevisionResult, BackendError>;
}
