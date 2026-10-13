// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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

/// 连接信息，用于预检查的源端/目标端配置
pub struct ConnectionInfo {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

/// 预检查请求参数
pub struct PrecheckParams {
    pub source: ConnectionInfo,
    pub target: ConnectionInfo,
    pub tables: Vec<(String, String)>, // (source_table, target_table)
}

/// 执行预检查：测试源端/目标端连接 + 验证表是否存在
pub async fn run_precheck(params: &PrecheckParams) -> PrecheckResult {
    let source_start = std::time::Instant::now();
    let source = check_connection(&params.source, "源端").await;
    let source_latency = source_start.elapsed().as_millis() as u64;

    let target_start = std::time::Instant::now();
    let target = check_connection(&params.target, "目标端").await;
    let target_latency = target_start.elapsed().as_millis() as u64;

    let mut table_results = Vec::new();

    // 检查表是否存在（仅当两端都可达时）
    if source.reachable && target.reachable {
        for (src_table, tgt_table) in &params.tables {
            let src_exists = check_table_exists(&params.source, src_table).await;
            let tgt_exists = check_table_exists(&params.target, tgt_table).await;

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
            name: params.source.db_type.clone(),
            db_type: params.source.db_type.clone(),
            reachable: source.reachable,
            message: source.message,
            latency_ms: source_latency,
        },
        target: PrecheckItem {
            name: params.target.db_type.clone(),
            db_type: params.target.db_type.clone(),
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

async fn check_connection(conn: &ConnectionInfo, label: &str) -> ConnCheck {
    let result = tokio::time::timeout(Duration::from_secs(10), try_connect(conn)).await;

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

async fn try_connect(conn: &ConnectionInfo) -> anyhow::Result<()> {
    match conn.db_type.to_lowercase().as_str() {
        "mysql" => {
            let opts = MySqlConnectOptions::from_str(&format!(
                "mysql://{user}:{password}@{host}:{port}/{database}",
                user = conn.user,
                password = conn.password,
                host = conn.host,
                port = conn.port,
                database = conn.database,
            ))?;
            let pool = sqlx::MySqlPool::connect_with(opts).await?;
            pool.close().await;
        },
        "postgresql" | "pg" | "postgres" => {
            let opts = PgConnectOptions::from_str(&format!(
                "postgresql://{user}:{password}@{host}:{port}/{database}",
                user = conn.user,
                password = conn.password,
                host = conn.host,
                port = conn.port,
                database = conn.database,
            ))?;
            let pool = sqlx::PgPool::connect_with(opts).await?;
            pool.close().await;
        },
        _ => {
            anyhow::bail!("不支持的数据库类型: {}", conn.db_type)
        },
    }
    Ok(())
}

async fn check_table_exists(conn: &ConnectionInfo, table_name: &str) -> bool {
    let result =
        tokio::time::timeout(Duration::from_secs(5), try_check_table(conn, table_name)).await;

    match result {
        Ok(Ok(exists)) => exists,
        _ => false,
    }
}

async fn try_check_table(conn: &ConnectionInfo, table_name: &str) -> anyhow::Result<bool> {
    match conn.db_type.to_lowercase().as_str() {
        "mysql" => {
            let opts = MySqlConnectOptions::from_str(&format!(
                "mysql://{user}:{password}@{host}:{port}/{database}",
                user = conn.user,
                password = conn.password,
                host = conn.host,
                port = conn.port,
                database = conn.database,
            ))?;
            let pool = sqlx::MySqlPool::connect_with(opts).await?;
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = ? AND table_name = ?"
            )
            .bind(&conn.database)
            .bind(table_name)
            .fetch_one(&pool)
            .await?;
            pool.close().await;
            Ok(row.0 > 0)
        },
        "postgresql" | "pg" | "postgres" => {
            let opts = PgConnectOptions::from_str(&format!(
                "postgresql://{user}:{password}@{host}:{port}/{database}",
                user = conn.user,
                password = conn.password,
                host = conn.host,
                port = conn.port,
                database = conn.database,
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
