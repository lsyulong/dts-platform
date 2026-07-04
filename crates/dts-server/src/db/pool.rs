use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

/// 创建 SQLite 连接池
pub async fn create_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    // 确保 data 目录存在
    if database_url.starts_with("sqlite:./") {
        let data_dir = database_url
            .strip_prefix("sqlite:./")
            .and_then(|p| {
                let path = std::path::Path::new(p);
                path.parent()
            })
            .unwrap_or(std::path::Path::new("./data"));

        if !data_dir.as_os_str().is_empty() {
            tokio::fs::create_dir_all(data_dir).await?;
        }
    }

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;

    log::info!("SQLite 连接池已创建");

    Ok(pool)
}
