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
use dts_backend::{BackendError, repo::MonitoringRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgMonitoringRepo {
    pool: PgPool,
}

impl PgMonitoringRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MonitoringRepo for PgMonitoringRepo {
    async fn get_overview(&self, tenant_id: &str) -> Result<MonitoringOverview, BackendError> {
        let total_tasks: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

        let running_tasks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1 AND status = 'running'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let failed_tasks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1 AND status = 'failed'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let total_datasources: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

        let healthy_datasources: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM datasource WHERE tenant_id = $1 AND health_status = 'healthy'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let today_synced_rows: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(rows_synced) FROM task_run tr
             JOIN sync_task st ON tr.task_id = st.id
             WHERE st.tenant_id = $1 AND tr.start_time >= CURRENT_DATE",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let today_synced_bytes: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(bytes_synced) FROM task_run tr
             JOIN sync_task st ON tr.task_id = st.id
             WHERE st.tenant_id = $1 AND tr.start_time >= CURRENT_DATE",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let recent_runs_rows = sqlx::query(
            "SELECT tr.id, tr.task_id, st.name, tr.status,
             tr.start_time, tr.rows_synced, tr.bytes_synced
             FROM task_run tr
             JOIN sync_task st ON tr.task_id = st.id
             WHERE st.tenant_id = $1
             ORDER BY tr.start_time DESC LIMIT 10",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let recent_runs = recent_runs_rows
            .into_iter()
            .map(|r| {
                let start_time: chrono::DateTime<chrono::Utc> = r.get(4);
                TaskRunInfo {
                    id: r.get(0),
                    status: r.get(3),
                    start_time: start_time.to_rfc3339(),
                    end_time: None,
                    rows_synced: r.get::<i64, _>(5),
                    bytes_synced: r.get::<i64, _>(6),
                    total_rows: None,
                    total_bytes: None,
                    error_message: None,
                }
            })
            .collect();

        Ok(MonitoringOverview {
            total_tasks,
            running_tasks,
            failed_tasks,
            total_datasources,
            healthy_datasources,
            today_synced_rows: today_synced_rows.unwrap_or(0),
            today_synced_bytes: today_synced_bytes.unwrap_or(0),
            success_rate: if total_tasks > 0 {
                100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
            } else {
                100.0
            },
            recent_runs,
        })
    }
}
