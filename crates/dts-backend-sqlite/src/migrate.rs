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
use dts_backend::BackendError;
use dts_backend_rdbms::MigrationRunner;
use sqlx::SqlitePool;

/// SQLite 迁移执行器
pub struct SqliteMigrationRunner {
    pool: SqlitePool,
}

impl SqliteMigrationRunner {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MigrationRunner for SqliteMigrationRunner {
    async fn run_migrations(&self) -> Result<(), BackendError> {
        let sql = include_str!("../migrations/001_initial_schema.sql");

        // SQLite doesn't support multiple statements in one execute call.
        // Split by semicolons and execute each non-empty, non-comment statement.
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }
            sqlx::query(trimmed)
                .execute(&self.pool)
                .await
                .map_err(|e| BackendError::Migration(format!("SQLite 迁移失败: {e}")))?;
        }

        tracing::info!("SQLite 数据库迁移完成");
        Ok(())
    }
}
