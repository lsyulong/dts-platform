use async_trait::async_trait;
use dts_backend::{BackendError, repo::RoleRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;

pub struct SqliteRoleRepo {
    pool: SqlitePool,
}

impl SqliteRoleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
