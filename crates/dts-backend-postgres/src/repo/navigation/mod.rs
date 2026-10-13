use async_trait::async_trait;
use dts_backend::{BackendError, repo::navigation::*};
use sqlx::{PgPool, Postgres, Transaction};
mod authority;
mod mutations;
mod storage;

pub struct PgNavigationRepo {
    pool: PgPool,
}
impl PgNavigationRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn read(&self) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?;
        Ok(tx)
    }
    async fn write(&self, tenant: &str) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let actual: Option<String> =
            sqlx::query_scalar("SELECT id FROM tenant WHERE id=$1 FOR UPDATE")
                .bind(tenant)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage::error)?;
        if actual.is_none() {
            return Err(BackendError::NotFound("导航租户不存在".into()));
        }
        Ok(tx)
    }
}
#[async_trait]
impl NavigationRepo for PgNavigationRepo {
    async fn assign(
        &self,
        p: &AssignNavigationParams,
    ) -> Result<NavigationDecisionResult, BackendError> {
        let semantics = validation::assign_semantics(p)?;
        let mut tx = self.write(&p.scope.tenant_id).await?;
        authority::manager(&mut tx, &p.scope, true).await?;
        let result = mutations::assign(&mut tx, p, &semantics).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn revoke(
        &self,
        p: &RevokeNavigationParams,
    ) -> Result<NavigationDecisionResult, BackendError> {
        let semantics = validation::revoke_semantics(p)?;
        let mut tx = self.write(&p.scope.tenant_id).await?;
        authority::manager(&mut tx, &p.scope, true).await?;
        let result = mutations::revoke(&mut tx, p, &semantics).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn current(&self, p: &ReadNavigationParams) -> Result<NavigationView, BackendError> {
        validation::scope(&p.scope)?;
        let mut tx = self.read().await?;
        let permissions = authority::permissions(&mut tx, &p.scope, false).await?;
        let assignment = storage::load(&mut tx, &p.scope.tenant_id, false).await?;
        let result = effective_navigation(
            p,
            &permissions,
            assignment.as_ref(),
            storage::now(&mut tx).await?,
        )?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn history(
        &self,
        p: &NavigationHistoryFilter,
    ) -> Result<NavigationHistoryPage, BackendError> {
        validation::scope(&p.scope)?;
        if !(1..=100).contains(&p.limit) || p.offset > 1_000_000 {
            return Err(storage::invalid("导航历史分页无效"));
        }
        let mut tx = self.read().await?;
        authority::manager(&mut tx, &p.scope, false).await?;
        let result = storage::history(&mut tx, p).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
}
