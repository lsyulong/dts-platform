// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dts_engine::{EngineError, EngineService, RunStatus, SyncProgress, TaskSubmission};
use tracing::{error, info};

use crate::client::SwanLakeClient;
use crate::sql;

type CancelMap = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

#[derive(Clone, Default)]
pub struct SwanLakeEngineService {
    cancellations: CancelMap,
}

impl SwanLakeEngineService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_count(&self) -> usize {
        self.cancellations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }
}

#[async_trait]
impl EngineService for SwanLakeEngineService {
    fn supports(&self, task: &TaskSubmission) -> bool {
        let source = task.source.db_type.trim().to_ascii_lowercase();
        let target = task.target.db_type.trim().to_ascii_lowercase();
        matches!(target.as_str(), "duckdb" | "swanlake")
            && matches!(
                source.as_str(),
                "mysql" | "pg" | "postgres" | "postgresql" | "duckdb" | "swanlake"
            )
    }

    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError> {
        task.config.validate()?;
        if !self.supports(&task) {
            return Err(EngineError::InvalidConfig(
                "SwanLake 引擎仅支持 MySQL/PostgreSQL/DuckDB 到 DuckDB 的全量同步".into(),
            ));
        }
        let source_kind = task.source.db_type.trim().to_ascii_lowercase();
        if matches!(source_kind.as_str(), "duckdb" | "swanlake")
            && (task.source.host != task.target.host || task.source.port != task.target.port)
        {
            return Err(EngineError::InvalidConfig(
                "DuckDB 源端和目标端必须位于同一个 SwanLake 会话端点".into(),
            ));
        }
        let endpoint = sql::endpoint(&task.target)?;
        let run_id = task.run_id.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut active = self
                .cancellations
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if active.insert(run_id.clone(), cancelled.clone()).is_some() {
                return Err(EngineError::InvalidConfig("PipelineRun ID 已在执行".into()));
            }
        }
        let active = self.cancellations.clone();
        let spawned_run = run_id.clone();
        tokio::spawn(async move {
            let result = execute(task.clone(), endpoint, cancelled, &progress_tx).await;
            if let Err(ref cause) = result {
                if !matches!(cause, EngineError::Cancelled(_)) {
                    error!(run_id = %spawned_run, error_kind = error_kind(cause), "SwanLake synchronization failed");
                    let _ = progress_tx.send(progress(
                        &task,
                        RunStatus::Failed,
                        0,
                        None,
                        0.0,
                        Some("SwanLake 同步失败，请检查受控服务日志".into()),
                    ));
                }
            }
            active
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&spawned_run);
        });
        Ok(run_id)
    }

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError> {
        let flag = self
            .cancellations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(run_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(run_id.into()))?;
        flag.store(true, Ordering::Release);
        Ok(())
    }
}

async fn execute(
    task: TaskSubmission,
    endpoint: String,
    cancelled: Arc<AtomicBool>,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<SyncProgress>,
) -> Result<(), EngineError> {
    let mut client = SwanLakeClient::connect(endpoint).await?;
    if let Some(attach) = sql::attach_source(&task.source)? {
        client
            .update(attach)
            .await
            .map_err(|_| EngineError::SourceConnection("SwanLake 无法附加源数据库".into()))?;
    }

    let mut tables = Vec::with_capacity(task.config.table_mappings.len());
    let mut total_rows = 0_u64;
    for (index, mapping) in task.config.table_mappings.iter().enumerate() {
        check_cancelled(&task, &cancelled, progress_tx, 0, total_rows).await?;
        let source = sql::source_table(&task.source, mapping)?;
        let target = sql::target_table(&task.target, mapping)?;
        let target_schema = sql::target_schema(&task.target, mapping)?;
        let stage = sql::quote_identifier(&format!("dts_stage_{index}"));
        client
            .update(format!(
                "CREATE OR REPLACE TEMP TABLE {stage} AS SELECT * FROM {source}"
            ))
            .await
            .map_err(|_| EngineError::SourceConnection("创建源表一致性快照失败".into()))?;
        let rows = client
            .scalar_u64(format!("SELECT COUNT(*) FROM {stage}"))
            .await
            .map_err(|_| EngineError::SourceConnection("源表计数失败".into()))?;
        total_rows = total_rows
            .checked_add(rows)
            .ok_or_else(|| EngineError::Internal("同步行数超出计数范围".into()))?;
        tables.push(StagedTable {
            stage,
            target,
            target_schema,
            rows,
            display_name: mapping.target_table.clone(),
        });
    }

    let mut copied = 0_u64;
    progress_tx
        .send(progress(
            &task,
            RunStatus::Running,
            copied,
            Some(total_rows),
            percent(copied, total_rows),
            Some("已完成源表检查，开始写入 DuckDB".into()),
        ))
        .map_err(|_| EngineError::Internal("progress receiver disconnected".into()))?;

    client.update("BEGIN TRANSACTION".into()).await?;
    let copy_result = copy_staged_tables(
        &mut client,
        &task,
        &cancelled,
        progress_tx,
        &tables,
        total_rows,
        &mut copied,
    )
    .await;
    if let Err(error) = copy_result {
        let _ = client.update("ROLLBACK".into()).await;
        return Err(error);
    }
    if let Err(error) = check_cancelled(&task, &cancelled, progress_tx, copied, total_rows).await {
        let _ = client.update("ROLLBACK".into()).await;
        return Err(error);
    }
    if let Err(error) = client.update("COMMIT".into()).await {
        let _ = client.update("ROLLBACK".into()).await;
        return Err(error);
    }
    for table in &tables {
        client.update(format!("DROP TABLE {}", table.stage)).await?;
    }

    info!(run_id = %task.run_id, rows = copied, "SwanLake synchronization completed");
    progress_tx
        .send(progress(
            &task,
            RunStatus::Completed,
            copied,
            Some(total_rows),
            100.0,
            Some("DuckDB 全量同步完成".into()),
        ))
        .map_err(|_| EngineError::Internal("progress receiver disconnected".into()))?;
    Ok(())
}

struct StagedTable {
    stage: String,
    target: String,
    target_schema: String,
    rows: u64,
    display_name: String,
}

async fn copy_staged_tables(
    client: &mut SwanLakeClient,
    task: &TaskSubmission,
    cancelled: &AtomicBool,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    tables: &[StagedTable],
    total_rows: u64,
    copied: &mut u64,
) -> Result<(), EngineError> {
    for table in tables {
        check_cancelled(task, cancelled, progress_tx, *copied, total_rows).await?;
        client
            .update(format!(
                "CREATE SCHEMA IF NOT EXISTS {}",
                table.target_schema
            ))
            .await?;
        client
            .update(format!(
                "CREATE TABLE IF NOT EXISTS {} AS SELECT * FROM {} WHERE FALSE",
                table.target, table.stage
            ))
            .await?;
        client
            .update(format!("DELETE FROM {}", table.target))
            .await?;
        let mut offset = 0_u64;
        while offset < table.rows {
            check_cancelled(task, cancelled, progress_tx, *copied, total_rows).await?;
            let remaining = table.rows - offset;
            let rows = remaining.min(task.config.batch_size as u64);
            client
                .update(format!(
                    "INSERT INTO {} SELECT * FROM {} LIMIT {rows} OFFSET {offset}",
                    table.target, table.stage
                ))
                .await?;
            offset += rows;
            *copied += rows;
            progress_tx
                .send(SyncProgress {
                    current_table: Some(table.display_name.clone()),
                    ..progress(
                        task,
                        RunStatus::Running,
                        *copied,
                        Some(total_rows),
                        percent(*copied, total_rows),
                        Some(format!("已写入 {}/{total_rows} 行", *copied)),
                    )
                })
                .map_err(|_| EngineError::Internal("progress receiver disconnected".into()))?;
        }
    }
    Ok(())
}

async fn check_cancelled(
    task: &TaskSubmission,
    cancelled: &AtomicBool,
    progress_tx: &tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    copied: u64,
    total: u64,
) -> Result<(), EngineError> {
    if progress_tx.is_closed() {
        return Err(EngineError::Internal(
            "progress receiver disconnected".into(),
        ));
    }
    if !cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    let _ = progress_tx.send(progress(
        task,
        RunStatus::Cancelled,
        copied,
        Some(total),
        percent(copied, total),
        Some("任务已取消".into()),
    ));
    Err(EngineError::Cancelled(task.run_id.clone()))
}

fn progress(
    task: &TaskSubmission,
    status: RunStatus,
    rows: u64,
    total: Option<u64>,
    percent: f64,
    message: Option<String>,
) -> SyncProgress {
    SyncProgress {
        run_id: task.run_id.clone(),
        task_id: task.task_id.clone(),
        status,
        rows_synced: rows,
        bytes_synced: 0,
        metrics_available: false,
        total_rows: total,
        current_table: None,
        percent,
        message,
    }
}

fn percent(copied: u64, total: u64) -> f64 {
    if total == 0 {
        100.0
    } else {
        copied as f64 * 100.0 / total as f64
    }
}

fn error_kind(error: &EngineError) -> &'static str {
    match error {
        EngineError::Internal(_) => "internal",
        EngineError::NotFound(_) => "not_found",
        EngineError::Cancelled(_) => "cancelled",
        EngineError::SourceConnection(_) => "source_connection",
        EngineError::TargetConnection(_) => "target_connection",
        EngineError::InvalidConfig(_) => "invalid_config",
    }
}
