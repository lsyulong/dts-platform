use async_trait::async_trait;
use dts_backend::{BackendError, repo::AlertRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteAlertRepo {
    pool: SqlitePool,
}

impl SqliteAlertRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
