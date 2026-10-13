use async_trait::async_trait;
use dts_backend::{BackendError, repo::standard_governance::*};
use sqlx::{PgPool, Postgres, Transaction};
use storage::*;

mod authority;
mod bindings;
mod dependencies;
mod gates;
mod inspection;
mod physical;
mod storage;
mod transitions;

pub struct PgStandardGovernanceRepo {
    pool: PgPool,
}
impl PgStandardGovernanceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn begin_write(&self, tenant: &str) -> Result<Transaction<'_, Postgres>, BackendError> {
        text(tenant)?;
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
    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        Ok(tx)
    }
}
#[async_trait]
impl StandardGovernanceRepo for PgStandardGovernanceRepo {
    async fn transition(
        &self,
        p: &StandardTransitionParams,
    ) -> Result<StandardTransitionResult, BackendError> {
        transitions::transition(self, p).await
    }
    async fn revision(
        &self,
        tenant: &str,
        standard_kind: &str,
        id: &str,
        revision_version: i64,
    ) -> Result<Option<StandardRevision>, BackendError> {
        text(tenant)?;
        kind(standard_kind)?;
        text(id)?;
        version(revision_version)?;
        let mut conn = self.pool.acquire().await.map_err(error)?;
        storage::revision(&mut conn, tenant, standard_kind, id, revision_version).await
    }
    async fn revisions(
        &self,
        p: &StandardRevisionFilter,
    ) -> Result<StandardRevisionPage, BackendError> {
        kind(&p.kind)?;
        text(&p.id)?;
        page(p.limit, p.offset)?;
        let mut tx = self.begin_read().await?;
        authority::require(&mut tx, &p.tenant_id, &p.actor_id, &["standards:read"]).await?;
        resource(&mut tx, &p.tenant_id, &p.kind, &p.id).await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM standard_revision WHERE tenant_id=$1 AND kind=$2 AND id=$3",
        )
        .bind(&p.tenant_id)
        .bind(&p.kind)
        .bind(&p.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(error)?;
        let versions:Vec<i64>=sqlx::query_scalar("SELECT revision_version FROM standard_revision WHERE tenant_id=$1 AND kind=$2 AND id=$3 ORDER BY revision_version DESC LIMIT $4 OFFSET $5").bind(&p.tenant_id).bind(&p.kind).bind(&p.id).bind(i64::from(p.limit)).bind(i64::from(p.offset)).fetch_all(&mut *tx).await.map_err(error)?;
        let mut data = Vec::new();
        for version in versions {
            data.push(
                storage::revision(&mut tx, &p.tenant_id, &p.kind, &p.id, version)
                    .await?
                    .ok_or_else(|| conflict("标准修订在读取过程中已失效"))?,
            );
        }
        tx.commit().await.map_err(error)?;
        Ok(StandardRevisionPage { data, total })
    }
    async fn bindings(
        &self,
        p: &StandardConsumerLookup,
    ) -> Result<Option<ModelStandardBindingSet>, BackendError> {
        text(&p.model_id)?;
        let mut tx = self.begin_read().await?;
        authority::require(
            &mut tx,
            &p.tenant_id,
            &p.actor_id,
            &["models:read", "standards:read"],
        )
        .await?;
        resource(&mut tx, &p.tenant_id, "data_model", &p.model_id).await?;
        let value = binding(&mut tx, &p.tenant_id, &p.model_id).await?;
        tx.commit().await.map_err(error)?;
        Ok(value)
    }
    async fn bind_model(
        &self,
        p: &BindModelStandardsParams,
    ) -> Result<BindModelStandardsResult, BackendError> {
        bindings::bind(self, p).await
    }
    async fn begin_model_publication(
        &self,
        p: &BeginStandardModelPublicationParams,
    ) -> Result<StandardModelPublicationClaimRecord, BackendError> {
        gates::begin(self, p).await
    }
    async fn finish_model_publication(
        &self,
        p: &FinishStandardModelPublicationParams,
    ) -> Result<StandardModelPublicationResult, BackendError> {
        gates::finish(self, p).await
    }
    async fn inspection_snapshot(
        &self,
        p: &StandardInspectionParams,
    ) -> Result<StandardInspectionSnapshot, BackendError> {
        let mut tx = self.begin_read().await?;
        let result = inspection::snapshot(&mut tx, p).await?;
        tx.commit().await.map_err(error)?;
        Ok(result)
    }
    async fn record_inspection(
        &self,
        p: &RecordStandardInspectionParams,
    ) -> Result<StandardInspectionResult, BackendError> {
        inspection::record(self, p).await
    }
}
