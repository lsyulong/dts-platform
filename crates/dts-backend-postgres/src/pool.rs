use async_trait::async_trait;
use dts_backend::{BackendError, BackendPool};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::str::FromStr;

/// PostgreSQL 连接池适配器
#[derive(Clone)]
pub struct PgBackendPool {
    pool: PgPool,
}

impl PgBackendPool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 获取内部 PgPool（供 Repository 实现使用）
    pub fn inner(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl BackendPool for PgBackendPool {
    fn backend_name(&self) -> &'static str {
        "postgres"
    }

    async fn ping(&self) -> Result<(), BackendError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

/// 创建 PostgreSQL 连接池
pub async fn create_pool(database_url: &str) -> Result<PgBackendPool, BackendError> {
    let options = PgConnectOptions::from_str(database_url)
        .map_err(|e| BackendError::Configuration(format!("无效的数据库 URL: {e}")))?
        .application_name("dts-platform");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .map_err(map_sqlx_error)?;

    tracing::info!("PostgreSQL 连接池已创建");
    Ok(PgBackendPool::new(pool))
}
