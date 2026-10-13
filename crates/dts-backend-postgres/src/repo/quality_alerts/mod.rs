use async_trait::async_trait;
use dts_backend::{BackendError, repo::quality_alerts::*};
use sqlx::{PgPool, Postgres, Transaction};
use storage::*;

mod reads;
mod storage;
mod verify;
mod writes;

pub struct PgQualityAlertRepo {
    pool: PgPool,
}
impl PgQualityAlertRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn begin_write(&self, tenant: &str) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM tenant WHERE id=$1 FOR UPDATE")
                .bind(tenant)
                .fetch_optional(&mut *tx)
                .await
                .map_err(error)?;
        if exists.is_none() {
            return Err(BackendError::NotFound("租户不存在".into()));
        }
        Ok(tx)
    }
}
#[async_trait]
impl QualityAlertRepo for PgQualityAlertRepo {
    async fn replay_evaluation(
        &self,
        tenant: &str,
        actor: &str,
        monitor: &str,
        version: i64,
        key: &str,
    ) -> Result<Option<QualityEvaluation>, BackendError> {
        let semantics = validation::evaluation_semantics(tenant, actor, monitor, version, key)?;
        let mut tx = self.begin_write(tenant).await?;
        authority(&mut tx, tenant, actor, true).await?;
        let mut result =
            receipt::<QualityEvaluation>(&mut tx, tenant, actor, key, &semantics).await?;
        if let Some(result) = result.as_mut() {
            result.replayed = true;
        }
        tx.commit().await.map_err(error)?;
        Ok(result)
    }
    async fn record_evaluation(
        &self,
        p: &RecordQualityEvaluationParams,
    ) -> Result<QualityEvaluation, BackendError> {
        writes::record(self, p).await
    }
    async fn acknowledge(
        &self,
        p: &AcknowledgeQualityAlertParams,
    ) -> Result<QualityAlert, BackendError> {
        writes::acknowledge(self, p).await
    }
    async fn get_observation(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<Option<QualityObservation>, BackendError> {
        let mut conn = self.pool.acquire().await.map_err(error)?;
        observation(&mut conn, tenant, id).await
    }
    async fn get_alert(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<Option<QualityAlert>, BackendError> {
        let mut conn = self.pool.acquire().await.map_err(error)?;
        alert(&mut conn, tenant, id).await
    }
    async fn list_observations(
        &self,
        f: &QualityAlertFilter,
    ) -> Result<QualityPage<QualityObservation>, BackendError> {
        reads::observations(&self.pool, f).await
    }
    async fn list_alerts(
        &self,
        f: &QualityAlertFilter,
    ) -> Result<QualityPage<QualityAlert>, BackendError> {
        reads::alerts(&self.pool, f).await
    }
    async fn list_events(
        &self,
        t: &str,
        a: &str,
        l: u32,
        o: u32,
    ) -> Result<QualityPage<QualityAlertEvent>, BackendError> {
        reads::events(&self.pool, t, a, l, o).await
    }
    async fn list_notices(
        &self,
        t: &str,
        r: &str,
        u: bool,
        l: u32,
        o: u32,
    ) -> Result<QualityPage<QualityAlertNotice>, BackendError> {
        reads::notices(&self.pool, t, r, u, l, o).await
    }
    async fn mark_notice_read(
        &self,
        t: &str,
        r: &str,
        id: &str,
    ) -> Result<QualityAlertNotice, BackendError> {
        reads::mark_read(self, t, r, id).await
    }
}
