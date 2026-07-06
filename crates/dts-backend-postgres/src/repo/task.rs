use async_trait::async_trait;
use dts_backend::{BackendError, repo::TaskRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgTaskRepo {
    pool: PgPool,
}

impl PgTaskRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::postgres::PgRow) -> TaskInfo {
        TaskInfo {
            id: r.get(0),
            name: r.get(1),
            description: r.get(2),
            source_id: r.get(3),
            target_id: r.get(4),
            cron_expr: r.get(5),
            status: r.get(6),
            created_at: r.get(7),
        }
    }
}

#[async_trait]
impl TaskRepo for PgTaskRepo {
    async fn create(&self, params: &CreateTaskParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO sync_task (id, tenant_id, name, description, source_id, target_id,
             cron_expr, config_json, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(&params.source_id)
        .bind(&params.target_id)
        .bind(&params.cron_expr)
        .bind(&params.config_json)
        .bind(&params.created_by)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<TaskInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, name, description, source_id, target_id, cron_expr, status, created_at
             FROM sync_task WHERE tenant_id = $1
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.iter().map(Self::row_to_info).collect())
    }

    async fn find_detail_by_id(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<TaskDetail>, BackendError> {
        let row = sqlx::query(
            "SELECT t.id, t.name, t.description, t.source_id, t.target_id,
             t.cron_expr, t.status, t.config_json, t.created_by, t.created_at, t.updated_at,
             s.name as source_name, s.db_type as source_type,
             d.name as target_name, d.db_type as target_type
             FROM sync_task t
             JOIN datasource s ON t.source_id = s.id
             JOIN datasource d ON t.target_id = d.id
             WHERE t.id = $1 AND t.tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => {
                let created_at: chrono::DateTime<chrono::Utc> = r.get(9);
                let updated_at: chrono::DateTime<chrono::Utc> = r.get(10);
                let runs = self.find_recent_runs(id, 10).await?;

                Ok(Some(TaskDetail {
                    id: r.get(0),
                    name: r.get(1),
                    description: r.get(2),
                    source_id: r.get(3),
                    target_id: r.get(4),
                    source_name: r.get(11),
                    source_type: r.get(12),
                    target_name: r.get(13),
                    target_type: r.get(14),
                    cron_expr: r.get(5),
                    status: r.get(6),
                    config_json: r.get(7),
                    created_by: r.get(8),
                    created_at: created_at.to_rfc3339(),
                    updated_at: updated_at.to_rfc3339(),
                    recent_runs: runs,
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_status(
        &self,
        id: &str,
        tenant_id: &str,
        status: &str,
    ) -> Result<bool, BackendError> {
        let result = sqlx::query(
            "UPDATE sync_task SET status = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3",
        )
        .bind(status)
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)
    }

    async fn count_by_status(&self, tenant_id: &str, status: &str) -> Result<i64, BackendError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1 AND status = $2",
        )
        .bind(tenant_id)
        .bind(status)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn find_recent_runs(
        &self,
        task_id: &str,
        limit: i64,
    ) -> Result<Vec<TaskRunInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, status, start_time, end_time, rows_synced, bytes_synced,
             total_rows, total_bytes, error_message
             FROM task_run WHERE task_id = $1
             ORDER BY start_time DESC LIMIT $2",
        )
        .bind(task_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let runs = rows
            .into_iter()
            .map(|r| {
                let start: chrono::DateTime<chrono::Utc> = r.get(2);
                let end: Option<chrono::DateTime<chrono::Utc>> = r.get(3);
                TaskRunInfo {
                    id: r.get(0),
                    status: r.get(1),
                    start_time: start.to_rfc3339(),
                    end_time: end.map(|dt| dt.to_rfc3339()),
                    rows_synced: r.get(4),
                    bytes_synced: r.get(5),
                    total_rows: r.get(6),
                    total_bytes: r.get(7),
                    error_message: r.get(8),
                }
            })
            .collect();

        Ok(runs)
    }
}
