use async_trait::async_trait;
use dts_backend::{BackendError, repo::TenantRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteTenantRepo {
    pool: SqlitePool,
}

impl SqliteTenantRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
