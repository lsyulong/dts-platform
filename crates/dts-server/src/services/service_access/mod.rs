mod application;
mod credentials;
mod invocation;
mod subscription;
mod types;

pub use credentials::CredentialKey;
pub use types::{CallPermit, IssuedCredential};

use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::{
    ProductResourceRepo, UserRepo,
    repo::service_access::{ServiceAccessRepo, ServiceAuthority},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceAccessService {
    repo: Arc<dyn ServiceAccessRepo>,
    resources: Arc<dyn ProductResourceRepo>,
    users: Arc<dyn UserRepo>,
    key: CredentialKey,
}
impl ServiceAccessService {
    pub fn new(
        repo: Arc<dyn ServiceAccessRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        users: Arc<dyn UserRepo>,
        key: CredentialKey,
    ) -> Self {
        Self {
            repo,
            resources,
            users,
            key,
        }
    }
    async fn authority(&self, actor: &AuthIdentity) -> Result<ServiceAuthority, AppError> {
        let authority = self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?;
        if !authority.active {
            return Err(denied("服务应用用户已停用或不存在"));
        }
        Ok(authority)
    }
}
fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}
fn denied(message: impl Into<String>) -> AppError {
    AppError::Forbidden(message.into())
}

#[cfg(test)]
mod tests;
