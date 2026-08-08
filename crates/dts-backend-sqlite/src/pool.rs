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


use async_trait::async_trait;
use dts_backend::{BackendError, BackendPool};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// SQLite 连接池适配器
#[derive(Clone)]
pub struct SqliteBackendPool {
    pool: SqlitePool,
}

impl SqliteBackendPool {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 获取内部 SqlitePool（供 Repository 实现使用）
    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl BackendPool for SqliteBackendPool {
    fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    async fn ping(&self) -> Result<(), BackendError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

/// 创建 SQLite 连接池
pub async fn create_pool(database_url: &str) -> Result<SqliteBackendPool, BackendError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(map_sqlx_error)?;

    // 启用 WAL 模式和外键约束
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;

    tracing::info!("SQLite 连接池已创建");
    Ok(SqliteBackendPool::new(pool))
}
