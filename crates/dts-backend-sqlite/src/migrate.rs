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
use sqlx::{Executor, SqlitePool};

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
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|e| BackendError::Migration(format!("SQLite 迁移事务失败: {e}")))?;
        for sql in [
            include_str!("../migrations/001_initial_schema.sql"),
            include_str!("../migrations/002_product_resources.sql"),
            include_str!("../migrations/003_task_logs.sql"),
            include_str!("../migrations/004_product_external_refs.sql"),
            include_str!("../migrations/005_product_permissions.sql"),
            include_str!("../migrations/006_product_jobs.sql"),
            include_str!("../migrations/007_model_constraints.sql"),
            include_str!("../migrations/008_message_deliveries.sql"),
            include_str!("../migrations/009_master_data.sql"),
            include_str!("../migrations/010_approval_permissions.sql"),
            include_str!("../migrations/011_metadata_catalog.sql"),
            include_str!("../migrations/012_data_access.sql"),
            include_str!("../migrations/013_data_access_permissions.sql"),
            include_str!("../migrations/014_assets.sql"),
            include_str!("../migrations/015_service_access.sql"),
            include_str!("../migrations/016_service_approval_permissions.sql"),
            include_str!("../migrations/017_bi_shares.sql"),
            include_str!("../migrations/018_sensitive_findings.sql"),
            include_str!("../migrations/019_tenant_navigation.sql"),
            include_str!("../migrations/020_organization_members.sql"),
            include_str!("../migrations/021_quality_alerts.sql"),
            include_str!("../migrations/022_standard_governance.sql"),
            include_str!("../migrations/023_development_revisions.sql"),
            include_str!("../migrations/024_runtime_configuration.sql"),
            include_str!("../migrations/025_tag_assignments.sql"),
            include_str!("../migrations/026_alert_policy.sql"),
            include_str!("../migrations/027_alert_automation.sql"),
            include_str!("../migrations/028_datasource_probes.sql"),
            include_str!("../migrations/029_notification_connections.sql"),
        ] {
            (&mut *transaction)
                .execute(sql)
                .await
                .map_err(|e| BackendError::Migration(format!("SQLite 迁移失败: {e}")))?;
        }
        transaction
            .commit()
            .await
            .map_err(|e| BackendError::Migration(format!("SQLite 迁移提交失败: {e}")))?;

        tracing::info!("SQLite 数据库迁移完成");
        Ok(())
    }
}
