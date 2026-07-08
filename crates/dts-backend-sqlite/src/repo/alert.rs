use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteAlertRepo {
    pool: SqlitePool,
}

impl SqliteAlertRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
