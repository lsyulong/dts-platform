use async_trait::async_trait;
use dts_backend::{BackendError, repo::DatasourceRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteDatasourceRepo {
    pool: SqlitePool,
}

impl SqliteDatasourceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
