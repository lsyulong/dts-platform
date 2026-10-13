use async_trait::async_trait;
use dts_backend::{BackendError, ProductResource, ResourcePage, repo::service_access::*};
use sqlx::{Sqlite, SqlitePool, Transaction};

mod access_audit;
mod application;
mod audit;
mod authority;
mod calls;
mod credentials;
mod storage;
mod subscription;
#[cfg(test)]
mod tests;

pub struct SqliteServiceAccessRepo {
    pool: SqlitePool,
}
impl SqliteServiceAccessRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    async fn begin(&self) -> Result<Transaction<'_, Sqlite>, BackendError> {
        self.pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage::error)
    }
}
#[async_trait]
impl ServiceAccessRepo for SqliteServiceAccessRepo {
    async fn actor_authority(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<ServiceAuthority, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let value = authority::get(&mut tx, tenant, actor).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn save_application(
        &self,
        p: &SaveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError> {
        application::save(self, p).await
    }
    async fn get_application(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceApplication>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        authority::active(&mut tx, tenant, actor).await?;
        let value = application::get(&mut tx, tenant, id).await?;
        if let Some(app) = &value {
            authority::manage(&mut tx, tenant, actor, &app.owner_id).await?;
        }
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_applications(
        &self,
        f: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError> {
        application::list(self, f).await
    }
    async fn archive_application(
        &self,
        p: &ArchiveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError> {
        application::archive(self, p).await
    }
    async fn issue_credential(
        &self,
        p: &IssueServiceCredentialParams,
    ) -> Result<ServiceCredentialIssueResult, BackendError> {
        credentials::issue(self, p).await
    }
    async fn revoke_credential(
        &self,
        p: &RevokeServiceCredentialParams,
    ) -> Result<ServiceCredentialSummary, BackendError> {
        credentials::revoke(self, p).await
    }
    async fn list_credentials(
        &self,
        f: &ServiceCredentialFilter,
    ) -> Result<ServiceCredentialPage, BackendError> {
        credentials::list(self, f).await
    }
    async fn lookup_credential(
        &self,
        id: &str,
    ) -> Result<Option<ServiceCredentialRecord>, BackendError> {
        validation::bounded(id, 128)?;
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let value = credentials::lookup(&mut tx, id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn save_subscription(
        &self,
        p: &SaveServiceSubscriptionParams,
    ) -> Result<ProductResource, BackendError> {
        subscription::save(self, p).await
    }
    async fn get_subscription(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceSubscription>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        authority::active(&mut tx, tenant, actor).await?;
        let value = subscription::get(&mut tx, tenant, id).await?;
        if let Some(sub) = &value {
            authority::manage(&mut tx, tenant, actor, &sub.owner_id).await?;
        }
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_subscriptions(
        &self,
        f: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError> {
        subscription::list(self, f).await
    }
    async fn decide_subscription(
        &self,
        p: &DecideServiceSubscriptionParams,
    ) -> Result<ServiceSubscriptionDecisionResult, BackendError> {
        subscription::decide(self, p).await
    }
    async fn reserve_call(
        &self,
        p: &ReserveServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError> {
        calls::reserve(self, p).await
    }
    async fn reauthorize_call(&self, expected: &ServiceCallRecord) -> Result<(), BackendError> {
        let mut tx = self.begin().await?;
        calls::check(&mut tx, expected).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(())
    }
    async fn finish_call(
        &self,
        p: &FinishServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError> {
        calls::finish(self, p).await
    }
    async fn list_calls(&self, f: &ServiceCallFilter) -> Result<ServiceCallPage, BackendError> {
        audit::list(self, f).await
    }
}
