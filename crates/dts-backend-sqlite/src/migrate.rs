use async_trait::async_trait;
use dts_backend::BackendError;
use dts_backend_rdbms::MigrationRunner;
use sqlx::SqlitePool;

/// SQLite 迁移执行器
pub struct SqliteMigrationRunner {
    pool: SqlitePool,
}

impl SqliteMigrationRunner {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MigrationRunner for SqliteMigrationRunner {
    async fn run_migrations(&self) -> Result<(), BackendError> {
        let sql = include_str!("../migrations/001_initial_schema.sql");

        // SQLite doesn't support multiple statements in one execute call.
        // Split by semicolons and execute each non-empty, non-comment statement.
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }
            sqlx::query(trimmed)
                .execute(&self.pool)
                .await
                .map_err(|e| BackendError::Migration(format!("SQLite 迁移失败: {e}")))?;
        }

        log::info!("SQLite 数据库迁移完成");
        Ok(())
    }
}
