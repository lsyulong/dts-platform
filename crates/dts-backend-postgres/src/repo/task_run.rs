use async_trait::async_trait;
use dts_backend::BackendError;
use dts_backend::repo::task_run::{CreateTaskRunParams, TaskRunRepo, UpdateTaskRunParams};
use dts_backend::types::TaskRunInfo;
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgTaskRunRepo {
    pool: PgPool,
}

impl PgTaskRunRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::postgres::PgRow) -> TaskRunInfo {
        TaskRunInfo {
            id: r.get(0),
            status: r.get(1),
            start_time: r.get(2),
            end_time: r.get(3),
            rows_synced: r.get(4),
            bytes_synced: r.get(5),
            total_rows: r.get(6),
            total_bytes: r.get(7),
            error_message: r.get(8),
        }
    }
}

#[async_trait]
impl TaskRunRepo for PgTaskRunRepo {
    async fn create(&self, params: &CreateTaskRunParams) -> Result<String, BackendError> {
        sqlx::query("INSERT INTO task_run (id, task_id, status) VALUES ($1, $2, $3)")
            .bind(&params.id)
            .bind(&params.task_id)
            .bind(&params.status)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn update(&self, params: &UpdateTaskRunParams) -> Result<bool, BackendError> {
        let result = sqlx::query(
            "UPDATE task_run SET status = $1, rows_synced = $2, bytes_synced = $3,
             total_rows = $4, total_bytes = $5, end_time = $6, error_message = $7
             WHERE id = $8",
        )
        .bind(&params.status)
        .bind(params.rows_synced)
        .bind(params.bytes_synced)
        .bind(params.total_rows)
        .bind(params.total_bytes)
        .bind(&params.end_time)
        .bind(&params.error_message)
        .bind(&params.run_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_recent(
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

        Ok(rows.iter().map(Self::row_to_info).collect())
    }
}
