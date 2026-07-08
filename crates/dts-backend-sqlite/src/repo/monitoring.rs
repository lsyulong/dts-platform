use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteMonitoringRepo {
    pool: SqlitePool,
}

impl SqliteMonitoringRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
