use async_trait::async_trait;
use dts_backend::{BackendError, repo::SeedRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteSeedRepo {
    pool: SqlitePool,
}

impl SqliteSeedRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
