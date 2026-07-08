use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteTenantRepo {
    pool: SqlitePool,
}

impl SqliteTenantRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
