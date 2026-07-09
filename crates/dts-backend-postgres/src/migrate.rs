use async_trait::async_trait;
use dts_backend::BackendError;
use dts_backend_rdbms::MigrationRunner;
use sqlx::PgPool;

/// PostgreSQL 迁移执行器
pub struct PgMigrationRunner {
    pool: PgPool,
}

impl PgMigrationRunner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MigrationRunner for PgMigrationRunner {
    async fn run_migrations(&self) -> Result<(), BackendError> {
        sqlx::migrate!("../dts-server/migrations")
            .run(&self.pool)
            .await
            .map_err(|e| BackendError::Migration(format!("数据库迁移失败: {e}")))?;
        tracing::info!("PostgreSQL 数据库迁移完成");
        Ok(())
    }
}
