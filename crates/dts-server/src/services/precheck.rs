//! 任务预检查服务 — 创建同步任务前验证源端/目标端连通性

use std::time::Duration;

use serde::Serialize;
use sqlx::{mysql::MySqlConnectOptions, postgres::PgConnectOptions};
use std::str::FromStr;

#[derive(Debug, Serialize)]
pub struct PrecheckResult {
    pub source: PrecheckItem,
    pub target: PrecheckItem,
    pub tables: Vec<TableCheckResult>,
    pub all_passed: bool,
}

#[derive(Debug, Serialize)]
pub struct PrecheckItem {
    pub name: String,
    pub db_type: String,
    pub reachable: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct TableCheckResult {
    pub source_table: String,
    pub target_table: String,
    pub source_exists: bool,
    pub target_exists: bool,
    pub message: String,
}

/// 执行预检查：测试源端/目标端连接 + 验证表是否存在
pub async fn run_precheck(
    source_type: &str,
    source_host: &str,
    source_port: u16,
    source_user: &str,
    source_password: &str,
    source_database: &str,
    target_type: &str,
    target_host: &str,
    target_port: u16,
    target_user: &str,
    target_password: &str,
    target_database: &str,
    tables: &[(&str, &str)], // (source_table, target_table)
) -> PrecheckResult {
    let source_start = std::time::Instant::now();
    let source = check_connection(
        source_type,
        source_host,
        source_port,
        source_user,
        source_password,
        source_database,
        "源端",
    )
    .await;
    let source_latency = source_start.elapsed().as_millis() as u64;

    let target_start = std::time::Instant::now();
    let target = check_connection(
        target_type,
        target_host,
        target_port,
        target_user,
        target_password,
        target_database,
        "目标端",
    )
    .await;
    let target_latency = target_start.elapsed().as_millis() as u64;

    let mut table_results = Vec::new();

    // 检查表是否存在（仅当两端都可达时）
    if source.reachable && target.reachable {
        for &(src_table, tgt_table) in tables {
            let src_exists = check_table_exists(
                source_type,
                source_host,
                source_port,
                source_user,
                source_password,
                source_database,
                src_table,
            )
            .await;

            let tgt_exists = check_table_exists(
                target_type,
                target_host,
                target_port,
                target_user,
                target_password,
                target_database,
                tgt_table,
            )
            .await;

            table_results.push(TableCheckResult {
                source_table: src_table.to_string(),
                target_table: tgt_table.to_string(),
                source_exists: src_exists,
                target_exists: tgt_exists,
                message: match (src_exists, tgt_exists) {
                    (true, true) => "OK".to_string(),
                    (false, _) => format!("源表 {src_table} 不存在"),
                    (_, false) => format!("目标表 {tgt_table} 不存在"),
                },
            });
        }
    }

    let all_passed = source.reachable
        && target.reachable
        && table_results
            .iter()
            .all(|t| t.source_exists && t.target_exists);

    PrecheckResult {
        source: PrecheckItem {
            name: source_type.to_string(),
            db_type: source_type.to_string(),
            reachable: source.reachable,
            message: source.message,
            latency_ms: source_latency,
        },
        target: PrecheckItem {
            name: target_type.to_string(),
            db_type: target_type.to_string(),
            reachable: target.reachable,
            message: target.message,
            latency_ms: target_latency,
        },
        tables: table_results,
        all_passed,
    }
}

struct ConnCheck {
    reachable: bool,
    message: String,
}

async fn check_connection(
    db_type: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
    label: &str,
) -> ConnCheck {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        try_connect(db_type, host, port, user, password, database),
    )
    .await;

    match result {
        Ok(Ok(())) => ConnCheck {
            reachable: true,
            message: format!("{label} 连接成功"),
        },
        Ok(Err(e)) => ConnCheck {
            reachable: false,
            message: format!("{label} 连接失败: {e}"),
        },
        Err(_) => ConnCheck {
            reachable: false,
            message: format!("{label} 连接超时 (10s)"),
        },
    }
}

async fn try_connect(
    db_type: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> anyhow::Result<()> {
    match db_type.to_lowercase().as_str() {
        "mysql" => {
            let opts = MySqlConnectOptions::from_str(&format!(
                "mysql://{user}:{password}@{host}:{port}/{database}"
            ))?;
            let pool = sqlx::MySqlPool::connect_with(opts).await?;
            pool.close().await;
        },
        "postgresql" | "pg" | "postgres" => {
            let opts = PgConnectOptions::from_str(&format!(
                "postgresql://{user}:{password}@{host}:{port}/{database}"
            ))?;
            let pool = sqlx::PgPool::connect_with(opts).await?;
            pool.close().await;
        },
        _ => {
            anyhow::bail!("不支持的数据库类型: {db_type}")
        },
    }
    Ok(())
}

async fn check_table_exists(
    db_type: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
    table_name: &str,
) -> bool {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        try_check_table(db_type, host, port, user, password, database, table_name),
    )
    .await;

    match result {
        Ok(Ok(exists)) => exists,
        _ => false,
    }
}

async fn try_check_table(
    db_type: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
    table_name: &str,
) -> anyhow::Result<bool> {
    match db_type.to_lowercase().as_str() {
        "mysql" => {
            let opts = MySqlConnectOptions::from_str(&format!(
                "mysql://{user}:{password}@{host}:{port}/{database}"
            ))?;
            let pool = sqlx::MySqlPool::connect_with(opts).await?;
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = ? AND table_name = ?"
            )
            .bind(database)
            .bind(table_name)
            .fetch_one(&pool)
            .await?;
            pool.close().await;
            Ok(row.0 > 0)
        },
        "postgresql" | "pg" | "postgres" => {
            let opts = PgConnectOptions::from_str(&format!(
                "postgresql://{user}:{password}@{host}:{port}/{database}"
            ))?;
            let pool = sqlx::PgPool::connect_with(opts).await?;
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1"
            )
            .bind(table_name)
            .fetch_one(&pool)
            .await?;
            pool.close().await;
            Ok(row.0 > 0)
        },
        _ => Ok(false),
    }
}
