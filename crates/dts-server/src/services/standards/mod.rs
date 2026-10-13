mod bindings;
mod contracts;
mod dependencies;
mod gate;
mod inspection;
mod lifecycle;
mod validation;

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::auth_svc::AuthService,
};
pub use contracts::*;
pub(crate) use dependencies::{active_revision, model_dependencies};
pub use dts_backend::repo::standard_governance::{
    ModelStandardsState, StandardModelEngineOutcome, StandardModelPublicationResult,
};
use dts_backend::{ProductResource, ProductResourceRepo, repo::standard_governance::*};
pub use gate::ModelStandardPublishClaim;
use std::sync::Arc;

#[derive(Clone)]
pub struct StandardGovernanceService {
    pub(super) repo: Arc<dyn StandardGovernanceRepo>,
    pub(super) resources: Arc<dyn ProductResourceRepo>,
    pub(super) auth: AuthService,
}
impl StandardGovernanceService {
    pub fn new(
        repo: Arc<dyn StandardGovernanceRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        auth: AuthService,
    ) -> Self {
        Self {
            repo,
            resources,
            auth,
        }
    }
    pub(super) async fn authorize(
        &self,
        actor: &AuthIdentity,
        permissions: &[&str],
    ) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        for permission in permissions {
            self.auth.authorize(actor, permission).await?;
        }
        Ok(())
    }
    pub(super) async fn resource(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<ProductResource, AppError> {
        validation::identifier(id)?;
        if version < 1 {
            return Err(validation::bad("版本必须为正整数"));
        }
        let resource = self
            .resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("当前租户没有此定义".into()))?;
        if resource.version != version {
            return Err(AppError::Conflict("定义版本已变化，请刷新后重试".into()));
        }
        Ok(resource)
    }
    pub async fn freeze_reference(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<FrozenStandardRef, AppError> {
        self.authorize(actor, &["standards:read"]).await?;
        Ok(self
            .active_revision(&actor.tenant_id, kind, id, version)
            .await?
            .reference())
    }
    pub async fn verify_reference(
        &self,
        actor: &AuthIdentity,
        reference: &FrozenStandardRef,
    ) -> Result<StandardRevision, AppError> {
        self.authorize(actor, &["standards:read"]).await?;
        let revision = self
            .active_revision(
                &actor.tenant_id,
                &reference.kind,
                &reference.id,
                reference.revision_version,
            )
            .await?;
        if revision.reference() != *reference {
            return Err(AppError::Conflict("固定标准版本的证据不一致".into()));
        }
        Ok(revision)
    }
    pub(super) async fn active_revision(
        &self,
        tenant: &str,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<StandardRevision, AppError> {
        dependencies::active_revision(self.repo.as_ref(), tenant, kind, id, version).await
    }
}

#[cfg(test)]
mod tests;
