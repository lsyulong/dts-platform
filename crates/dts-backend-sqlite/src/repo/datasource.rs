use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteDatasourceRepo {
    pool: SqlitePool,
}

impl SqliteDatasourceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
