use async_trait::async_trait;
use dts_backend::{BackendError, BackendPool};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// SQLite 连接池适配器
#[derive(Clone)]
pub struct SqliteBackendPool {
    pool: SqlitePool,
}

impl SqliteBackendPool {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 获取内部 SqlitePool（供 Repository 实现使用）
    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl BackendPool for SqliteBackendPool {
    fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    async fn ping(&self) -> Result<(), BackendError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

/// 创建 SQLite 连接池
pub async fn create_pool(database_url: &str) -> Result<SqliteBackendPool, BackendError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(map_sqlx_error)?;

    // 启用 WAL 模式和外键约束
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;

    tracing::info!("SQLite 连接池已创建");
    Ok(SqliteBackendPool::new(pool))
}
