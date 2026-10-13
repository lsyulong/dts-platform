// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use dts_backend::{
    BackendError,
    repo::task_log::{AppendTaskLog, TaskLogPage, TaskLogRecord, TaskLogRepo},
};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

pub struct SqliteTaskLogRepo {
    pool: SqlitePool,
}
impl SqliteTaskLogRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn decode(row: &SqliteRow) -> Result<TaskLogRecord, BackendError> {
    Ok(TaskLogRecord {
        id: row.try_get("id").map_err(map_sqlx_error)?,
        sequence: row.try_get("sequence").map_err(map_sqlx_error)?,
        timestamp: row.try_get("timestamp").map_err(map_sqlx_error)?,
        level: row.try_get("level").map_err(map_sqlx_error)?,
        component: row.try_get("component").map_err(map_sqlx_error)?,
        message: row.try_get("message").map_err(map_sqlx_error)?,
    })
}

#[async_trait]
impl TaskLogRepo for SqliteTaskLogRepo {
    async fn append(&self, params: &AppendTaskLog) -> Result<TaskLogRecord, BackendError> {
        // A single SQLite write statement serializes sequence allocation with its insert.
        let row = sqlx::query("INSERT INTO task_run_log (id,tenant_id,task_id,run_id,sequence,timestamp,level,component,message)
            SELECT ?1,?2,?3,?4,COALESCE(MAX(l.sequence),0)+1,?5,?6,?7,?8
            FROM task_run r JOIN sync_task t ON t.id=r.task_id
            LEFT JOIN task_run_log l ON l.run_id=r.id
            WHERE r.id=?4 AND r.task_id=?3 AND t.tenant_id=?2 GROUP BY r.id
            RETURNING id,sequence,timestamp,level,component,message")
            .bind(&params.id).bind(&params.tenant_id).bind(&params.task_id).bind(&params.run_id)
            .bind(&params.timestamp).bind(&params.level).bind(&params.component).bind(&params.message)
            .fetch_optional(&self.pool).await.map_err(map_sqlx_error)?
            .ok_or_else(|| BackendError::NotFound("任务运行不存在".into()))?;
        decode(&row)
    }

    async fn list(
        &self,
        tenant_id: &str,
        task_id: &str,
        run_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<TaskLogPage, BackendError> {
        if offset < 0 || !(1..=200).contains(&limit) {
            return Err(BackendError::ConstraintViolation("日志分页范围无效".into()));
        }
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        let exists: Option<String> = sqlx::query_scalar("SELECT r.id FROM task_run r JOIN sync_task t ON t.id=r.task_id WHERE r.id=?1 AND r.task_id=?2 AND t.tenant_id=?3")
            .bind(run_id).bind(task_id).bind(tenant_id).fetch_optional(&mut *tx).await.map_err(map_sqlx_error)?;
        if exists.is_none() {
            return Err(BackendError::NotFound("任务运行不存在".into()));
        }
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_run_log WHERE tenant_id=?1 AND task_id=?2 AND run_id=?3",
        )
        .bind(tenant_id)
        .bind(task_id)
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        let rows = sqlx::query("SELECT id,sequence,timestamp,level,component,message FROM task_run_log WHERE tenant_id=?1 AND task_id=?2 AND run_id=?3 ORDER BY sequence ASC LIMIT ?4 OFFSET ?5")
            .bind(tenant_id).bind(task_id).bind(run_id).bind(limit).bind(offset).fetch_all(&mut *tx).await.map_err(map_sqlx_error)?;
        let data = rows.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(TaskLogPage { data, total })
    }
}
