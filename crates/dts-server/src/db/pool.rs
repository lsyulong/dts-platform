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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== create_pool with sqlite::memory Tests ==========

    #[tokio::test]
    async fn create_pool_with_memory_url_works() {
        let pool = create_pool("sqlite::memory:").await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();
        // Verify pool works by running a simple query
        let result: i64 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(result, 1);
    }

    #[tokio::test]
    async fn create_pool_creates_in_memory_db() {
        let pool = create_pool("sqlite::memory:")
            .await
            .expect("Failed to create pool");

        // Create a table and insert data to verify full functionality
        sqlx::query("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO test_table (id, name) VALUES (1, 'hello')")
            .execute(&pool)
            .await
            .unwrap();

        let name: String = sqlx::query_scalar("SELECT name FROM test_table WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "hello");
    }

    #[tokio::test]
    async fn create_pool_max_connections_is_set() {
        let pool = create_pool("sqlite::memory:")
            .await
            .expect("Failed to create pool");

        // Pool size should be configured (max 10)
        assert!(pool.size() <= 10);
    }

    #[tokio::test]
    async fn create_pool_twice_creates_independent_pools() {
        let pool1 = create_pool("sqlite::memory:")
            .await
            .expect("Failed to create pool1");
        let pool2 = create_pool("sqlite::memory:")
            .await
            .expect("Failed to create pool2");

        // Create table in pool1
        sqlx::query("CREATE TABLE t1 (x INTEGER)")
            .execute(&pool1)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t1 VALUES (42)")
            .execute(&pool1)
            .await
            .unwrap();

        // pool2 should NOT see pool1's data (separate memory DBs)
        let result = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master WHERE name='t1'")
            .fetch_one(&pool2)
            .await
            .unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn create_pool_clone_works() {
        let pool = create_pool("sqlite::memory:")
            .await
            .expect("Failed to create pool");
        let pool2 = pool.clone();

        sqlx::query("CREATE TABLE shared (val TEXT)")
            .execute(&pool)
            .await
            .unwrap();

        let result: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name='shared'")
            .fetch_one(&pool2)
            .await
            .unwrap();
        assert_eq!(result, 1);
    }
}
