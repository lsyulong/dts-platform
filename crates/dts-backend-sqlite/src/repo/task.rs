use async_trait::async_trait;
use dts_backend::{BackendError, repo::TaskRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteTaskRepo {
    pool: SqlitePool,
}

impl SqliteTaskRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
