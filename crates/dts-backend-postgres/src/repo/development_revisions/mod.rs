use async_trait::async_trait;
use dts_backend::{BackendError, ProductResource, repo::development_revisions::*};
use sqlx::{PgPool, Postgres, Transaction};
mod authority;
mod restore;
mod save;
mod storage;

pub struct PgDevelopmentRevisionRepo {
    pool: PgPool,
}
impl PgDevelopmentRevisionRepo {
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
    async fn begin_write(
        &self,
        s: &DevelopmentRevisionScope,
    ) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let tenant: Option<String> =
            sqlx::query_scalar("SELECT id FROM tenant WHERE id=$1 FOR UPDATE")
                .bind(&s.tenant_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage::error)?;
        if tenant.is_none() {
            return Err(BackendError::Forbidden("修订操作租户已失效".into()));
        }
        authority::check(&mut tx, s, true, true).await?;
        Ok(tx)
    }
}
#[async_trait]
impl DevelopmentRevisionRepo for PgDevelopmentRevisionRepo {
    async fn current(&self, s: &DevelopmentRevisionScope) -> Result<ProductResource, BackendError> {
        let mut tx = self.begin_read().await?;
        authority::check(&mut tx, s, false, false).await?;
        let resource = storage::source(&mut tx, s).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(resource)
    }
    async fn save(
        &self,
        p: &SaveDevelopmentRevisionParams,
    ) -> Result<SaveDevelopmentRevisionResult, BackendError> {
        validation::write(&p.scope, p.expected_version, &p.reason, &p.idempotency_key)?;
        let mut tx = self.begin_write(&p.scope).await?;
        let result = save::save(&mut tx, p).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn restore(
        &self,
        p: &RestoreDevelopmentRevisionParams,
    ) -> Result<RestoreDevelopmentRevisionResult, BackendError> {
        validation::write(&p.scope, p.expected_version, &p.reason, &p.idempotency_key)?;
        validation::identifier(&p.revision_id)?;
        let mut tx = self.begin_write(&p.scope).await?;
        let result = restore::restore(&mut tx, p).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn detail(
        &self,
        p: &ReadDevelopmentRevisionParams,
    ) -> Result<DevelopmentRevision, BackendError> {
        validation::identifier(&p.revision_id)?;
        let mut tx = self.begin_read().await?;
        authority::check(&mut tx, &p.scope, false, false).await?;
        storage::source(&mut tx, &p.scope).await?;
        let revision = storage::revision(&mut tx, &p.scope, &p.revision_id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(revision)
    }
    async fn history(
        &self,
        f: &DevelopmentRevisionFilter,
    ) -> Result<DevelopmentRevisionPage, BackendError> {
        validation::page(f.limit, f.offset)?;
        let mut tx = self.begin_read().await?;
        authority::check(&mut tx, &f.scope, false, false).await?;
        storage::source(&mut tx, &f.scope).await?;
        let total=sqlx::query_scalar("SELECT COUNT(*) FROM development_revision WHERE tenant_id=$1 AND source_kind=$2 AND source_id=$3")
            .bind(&f.scope.tenant_id).bind(&f.scope.kind).bind(&f.scope.resource_id).fetch_one(&mut *tx).await.map_err(storage::error)?;
        let rows=sqlx::query("SELECT id,tenant_id,source_kind,source_id,source_version,snapshot_hash,saved_by,saved_at,reason FROM development_revision WHERE tenant_id=$1 AND source_kind=$2 AND source_id=$3 ORDER BY source_version DESC,id LIMIT $4 OFFSET $5")
            .bind(&f.scope.tenant_id).bind(&f.scope.kind).bind(&f.scope.resource_id).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(&mut *tx).await.map_err(storage::error)?;
        let data = rows
            .iter()
            .map(storage::decode_summary)
            .collect::<Result<_, _>>()?;
        tx.commit().await.map_err(storage::error)?;
        Ok(DevelopmentRevisionPage { data, total })
    }
}
