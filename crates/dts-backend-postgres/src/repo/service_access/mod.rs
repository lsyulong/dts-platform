use async_trait::async_trait;
use dts_backend::{BackendError, ProductResource, ResourcePage, repo::service_access::*};
use sqlx::{PgPool, Postgres, Transaction};

mod access_audit;
mod application;
mod authority;
mod call_policy;
mod call_reads;
mod calls;
mod credentials;
mod decisions;
mod quota;
mod reads;
mod receipts;
mod storage;
mod subscription;

pub struct PgServiceAccessRepo {
    pool: PgPool,
}
impl PgServiceAccessRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?;
        Ok(tx)
    }
    async fn begin_write(&self, tenant: &str) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM tenant WHERE id=$1 FOR UPDATE")
                .bind(tenant)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage::error)?;
        if exists.is_none() {
            return Err(storage::invalid("服务租户不存在"));
        }
        Ok(tx)
    }
}

#[async_trait]
impl ServiceAccessRepo for PgServiceAccessRepo {
    async fn actor_authority(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<ServiceAuthority, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = authority::get(&mut tx, tenant, actor, false).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn save_application(
        &self,
        params: &SaveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = application::save(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn get_application(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceApplication>, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = reads::application(&mut tx, tenant, actor, id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_applications(
        &self,
        filter: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = reads::applications(&mut tx, filter).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn archive_application(
        &self,
        params: &ArchiveServiceApplicationParams,
    ) -> Result<ProductResource, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = application::archive(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn issue_credential(
        &self,
        params: &IssueServiceCredentialParams,
    ) -> Result<ServiceCredentialIssueResult, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = credentials::issue(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn revoke_credential(
        &self,
        params: &RevokeServiceCredentialParams,
    ) -> Result<ServiceCredentialSummary, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = credentials::revoke(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_credentials(
        &self,
        filter: &ServiceCredentialFilter,
    ) -> Result<ServiceCredentialPage, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = credentials::list(&mut tx, filter).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn lookup_credential(
        &self,
        id: &str,
    ) -> Result<Option<ServiceCredentialRecord>, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = credentials::lookup(&mut tx, id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn save_subscription(
        &self,
        params: &SaveServiceSubscriptionParams,
    ) -> Result<ProductResource, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = subscription::save(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn get_subscription(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<ServiceSubscription>, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = reads::subscription(&mut tx, tenant, actor, id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_subscriptions(
        &self,
        filter: &ServiceResourceFilter,
    ) -> Result<ResourcePage, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = reads::subscriptions(&mut tx, filter).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn decide_subscription(
        &self,
        params: &DecideServiceSubscriptionParams,
    ) -> Result<ServiceSubscriptionDecisionResult, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = decisions::decide(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn reserve_call(
        &self,
        params: &ReserveServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError> {
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let value = calls::reserve(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        match value {
            calls::ReservationOutcome::Allowed(value) => Ok(*value),
            calls::ReservationOutcome::Denied(error) => Err(error),
        }
    }
    async fn reauthorize_call(&self, expected: &ServiceCallRecord) -> Result<(), BackendError> {
        let mut tx = self.begin_write(&expected.tenant_id).await?;
        calls::reauthorize(&mut tx, expected).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(())
    }
    async fn finish_call(
        &self,
        params: &FinishServiceCallParams,
    ) -> Result<ServiceCallRecord, BackendError> {
        let mut tx = self.begin_write(&params.call.tenant_id).await?;
        let value = calls::finish(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list_calls(
        &self,
        filter: &ServiceCallFilter,
    ) -> Result<ServiceCallPage, BackendError> {
        let mut tx = self.begin_read().await?;
        let value = call_reads::list(&mut tx, filter).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
}
