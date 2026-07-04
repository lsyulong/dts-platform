pub mod pool;
pub mod seed;

use sqlx::SqlitePool;

pub use pool::create_pool;
pub use seed::seed_default_data;

/// 运行数据库迁移
pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    log::info!("数据库迁移完成");
    Ok(())
}
