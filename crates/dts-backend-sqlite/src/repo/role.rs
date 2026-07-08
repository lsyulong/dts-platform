use sqlx::SqlitePool;

#[allow(dead_code)]
pub struct SqliteRoleRepo {
    pool: SqlitePool,
}

impl SqliteRoleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
