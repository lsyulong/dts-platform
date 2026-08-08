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
use sqlx::PgPool;

/// PostgreSQL 迁移执行器
pub struct PgMigrationRunner {
    pool: PgPool,
}

impl PgMigrationRunner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MigrationRunner for PgMigrationRunner {
    async fn run_migrations(&self) -> Result<(), BackendError> {
        sqlx::migrate!("../dts-server/migrations")
            .run(&self.pool)
            .await
            .map_err(|e| BackendError::Migration(format!("数据库迁移失败: {e}")))?;
        tracing::info!("PostgreSQL 数据库迁移完成");
        Ok(())
    }
}
