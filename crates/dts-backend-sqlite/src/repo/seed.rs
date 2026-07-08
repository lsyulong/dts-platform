use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteSeedRepo {
    pool: SqlitePool,
}

impl SqliteSeedRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
