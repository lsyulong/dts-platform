use async_trait::async_trait;
use dts_backend::{BackendError, ProductResource, repo::runtime_configuration::*};
use sqlx::{Sqlite, SqlitePool, Transaction};
pub(crate) mod jobs;
mod lifecycle;
mod storage;

pub struct SqliteRuntimeConfigurationRepo {
    pool: SqlitePool,
}
impl SqliteRuntimeConfigurationRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    async fn write(&self, tenant: &str) -> Result<Transaction<'_, Sqlite>, BackendError> {
        validation::text(tenant, 128)?;
        self.pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage::error)
    }
}

#[async_trait]
impl RuntimeConfigurationRepo for SqliteRuntimeConfigurationRepo {
    async fn get(&self, p: &RuntimeConfigurationLookup) -> Result<ProductResource, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        storage::authorize(&mut tx, &p.scope, RuntimeSourceAccess::Read).await?;
        let result = storage::resource(
            &mut tx,
            &p.scope.tenant_id,
            "development_config",
            &p.configuration_id,
        )
        .await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn publish(
        &self,
        p: &PublishRuntimeConfigurationParams,
    ) -> Result<RuntimeConfigurationChange, BackendError> {
        lifecycle::change(
            self,
            &p.lookup,
            p.expected_version,
            &p.reason,
            &p.idempotency_key,
            p.validation.as_ref(),
            true,
        )
        .await
    }
    async fn withdraw(
        &self,
        p: &WithdrawRuntimeConfigurationParams,
    ) -> Result<RuntimeConfigurationChange, BackendError> {
        lifecycle::change(
            self,
            &p.lookup,
            p.expected_version,
            &p.reason,
            &p.idempotency_key,
            None,
            false,
        )
        .await
    }
    async fn current_source(
        &self,
        p: &RuntimeSourceLookup,
    ) -> Result<RuntimeSourceInput, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        let result = storage::source(
            &mut tx,
            p,
            matches!(p.access, RuntimeSourceAccess::Execute)
                || matches!(p.access, RuntimeSourceAccess::Read),
        )
        .await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn verify(
        &self,
        p: &VerifyRuntimeConfigurationParams,
    ) -> Result<RuntimeEvidenceState, BackendError> {
        let current = self.current_source(&p.source).await?;
        if current.evidence != p.evidence {
            return Err(storage::conflict("冻结运行配置已失效，不能替换成新版本"));
        }
        Ok(RuntimeEvidenceState {
            valid: true,
            reason_code: None,
            frozen_evidence_preserved: true,
        })
    }
    async fn publish_source(
        &self,
        p: &PublishRuntimeSourceParams,
    ) -> Result<ProductResource, BackendError> {
        let mut tx = self.write(&p.source.scope.tenant_id).await?;
        storage::authorize(&mut tx, &p.source.scope, RuntimeSourceAccess::Publish).await?;
        storage::authorize(&mut tx, &p.source.scope, RuntimeSourceAccess::Execute).await?;
        let current = storage::source(&mut tx, &p.source, false).await?;
        if !["draft", "inactive", "publish_failed"].contains(&current.source.status.as_str())
            || !p.validation.matches(&current.source)?
            || current.evidence != p.evidence
        {
            return Err(storage::conflict("来源或配置已改变，须重新校验发布"));
        }
        let result = storage::status(
            &mut tx,
            &p.source.scope,
            &current.source,
            "published",
            "publish_read_only_with_configuration",
        )
        .await?;
        if let Some(evidence) = current.evidence {
            sqlx::query("INSERT INTO runtime_source_configuration_binding(tenant_id,source_kind,source_id,source_version,source_hash,configuration_id,configuration_version,evidence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
                .bind(&result.tenant_id).bind(&result.kind).bind(&result.id).bind(result.version).bind(validation::resource_hash(&result)?).bind(&evidence.configuration_id).bind(evidence.configuration_version).bind(storage::encode(&evidence)?).execute(&mut *tx).await.map_err(storage::error)?;
        }
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
}
