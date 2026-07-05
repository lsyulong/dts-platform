pub mod pool;
pub mod seed;

use sqlx::PgPool;

pub use pool::create_pool;
pub use seed::seed_default_data;

/// 数据库连接池类型别名，方便后续切换数据库
pub type DbPool = PgPool;

/// 运行数据库迁移
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    log::info!("数据库迁移完成");
    Ok(())
}