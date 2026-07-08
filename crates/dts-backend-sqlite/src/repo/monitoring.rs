use async_trait::async_trait;
use dts_backend::{BackendError, repo::MonitoringRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteMonitoringRepo {
    pool: SqlitePool,
}

impl SqliteMonitoringRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
