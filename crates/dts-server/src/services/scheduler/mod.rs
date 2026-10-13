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

use std::sync::Arc;

use dashmap::{DashMap, mapref::entry::Entry};
use dts_backend::{
    BackendError,
    repo::{CreateTaskRunParams, DatasourceRepo, TaskRepo, TaskRunRepo, UpdateTaskRunParams},
};
use dts_engine::{ConnectionConfig, EngineService, RunStatus, SyncProgress, TaskSubmission};
use tracing::{error, info};
use uuid::Uuid;

use crate::services::task_log_svc::{RunLogScope, safe_engine_message};
use crate::{services::task_svc::PipelineConfig, ws::task_log::WsHub};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct TaskScheduler {
    engine: Arc<dyn EngineService>,
    task_repo: Arc<dyn TaskRepo>,
    task_run_repo: Arc<dyn TaskRunRepo>,
    datasource_repo: Arc<dyn DatasourceRepo>,
    ws_hub: Arc<WsHub>,
    active_runs: Arc<DashMap<String, String>>,
}

struct ActiveRunGuard {
    key: String,
    active: Arc<DashMap<String, String>>,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        self.active.remove(&self.key);
    }
}

impl TaskScheduler {
    pub fn new(
        engine: Arc<dyn EngineService>,
        task_repo: Arc<dyn TaskRepo>,
        task_run_repo: Arc<dyn TaskRunRepo>,
        datasource_repo: Arc<dyn DatasourceRepo>,
        ws_hub: Arc<WsHub>,
    ) -> Self {
        Self {
            engine,
            task_repo,
            task_run_repo,
            datasource_repo,
            ws_hub,
            active_runs: Arc::new(DashMap::new()),
        }
    }

    pub async fn start_task(&self, task_id: &str, tenant_id: &str) -> Result<String, BackendError> {
        let task = self
            .task_repo
            .find_detail_by_id(task_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound(format!("任务不存在: {task_id}")))?;
        if task.status == "running" {
            return Err(BackendError::AlreadyExists(
                "任务已在运行，或存在需要恢复的运行记录".into(),
            ));
        }
        let run_id = Uuid::new_v4().to_string();
        let key = format!("{tenant_id}/{task_id}");
        match self.active_runs.entry(key.clone()) {
            Entry::Occupied(_) => return Err(BackendError::AlreadyExists("任务已在运行".into())),
            Entry::Vacant(entry) => {
                entry.insert(run_id.clone());
            },
        }
        let reservation = ActiveRunGuard {
            key,
            active: self.active_runs.clone(),
        };
        let source = self
            .datasource_repo
            .get_connection_info(&task.source_id, tenant_id)
            .await?
            .ok_or_else(|| {
                BackendError::NotFound(format!("源端数据源不存在: {}", task.source_id))
            })?;
        let target = self
            .datasource_repo
            .get_connection_info(&task.target_id, tenant_id)
            .await?
            .ok_or_else(|| {
                BackendError::NotFound(format!("目标端数据源不存在: {}", task.target_id))
            })?;
        let submission = self.build_submission(&task, &source, &target, &run_id)?;
        let rx = self
            .persist_and_submit(task_id, tenant_id, &run_id, submission)
            .await?;
        let scheduler = self.clone();
        let tid = task_id.to_string();
        let tenant = tenant_id.to_string();
        let rid = run_id.clone();
        tokio::spawn(async move {
            let _reservation = reservation;
            scheduler.listen_progress(&tid, &tenant, &rid, rx).await;
        });
        info!(task_id, run_id, "PipelineRun submitted");
        Ok(run_id)
    }

    async fn persist_and_submit(
        &self,
        task_id: &str,
        tenant_id: &str,
        run_id: &str,
        submission: TaskSubmission,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<SyncProgress>, BackendError> {
        // Persist the run before an engine may emit progress or perform external writes.
        let persisted_id = self
            .task_run_repo
            .create(&CreateTaskRunParams {
                id: run_id.into(),
                task_id: task_id.into(),
                status: "running".into(),
            })
            .await?;
        if persisted_id != run_id {
            return Err(BackendError::Internal(
                "存储后端返回不一致的 PipelineRun ID".into(),
            ));
        }
        self.record_log(task_id, tenant_id, run_id, "INFO", "运行实例已建立")
            .await;
        let start_status = self
            .task_repo
            .update_status(task_id, tenant_id, "running")
            .await;
        if let Err(e) = require_update(start_status, "任务启动状态") {
            self.fail_run(task_id, tenant_id, run_id, e.to_string())
                .await;
            return Err(e);
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let submitted = self.engine.submit(submission, tx).await;
        match submitted {
            Ok(id) if id == run_id => {
                self.record_log(task_id, tenant_id, run_id, "INFO", "引擎已接受任务提交")
                    .await;
            },
            Ok(id) => {
                if let Err(e) = self.engine.cancel(&id).await {
                    error!(run_id = %id, error = %e, "cancel incompatible engine run failed");
                }
                let message = "引擎返回不一致的 PipelineRun ID".to_string();
                self.fail_run(task_id, tenant_id, run_id, message.clone())
                    .await;
                return Err(BackendError::Internal(message));
            },
            Err(e) => {
                self.fail_run(task_id, tenant_id, run_id, e.to_string())
                    .await;
                return Err(BackendError::Internal(format!("引擎提交失败: {e}")));
            },
        }
        Ok(rx)
    }

    pub async fn cancel_task(&self, task_id: &str, tenant_id: &str) -> Result<(), BackendError> {
        self.task_repo
            .find_detail_by_id(task_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound("任务不存在".into()))?;
        let key = format!("{tenant_id}/{task_id}");
        let run_id = self
            .active_runs
            .get(&key)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| BackendError::ConstraintViolation("任务没有可取消的活跃运行".into()))?;
        self.engine
            .cancel(&run_id)
            .await
            .map_err(|e| BackendError::Internal(format!("取消运行失败: {e}")))?;
        self.record_log(
            task_id,
            tenant_id,
            &run_id,
            "INFO",
            "已发送取消请求，等待引擎确认终态",
        )
        .await;
        Ok(())
    }

    async fn listen_progress(
        &self,
        task_id: &str,
        tenant_id: &str,
        run_id: &str,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<SyncProgress>,
    ) {
        while let Some(mut progress) = rx.recv().await {
            if progress.run_id != run_id || progress.task_id != task_id {
                self.cancel_and_fail(task_id, tenant_id, run_id, "引擎进度标识不匹配".into())
                    .await;
                return;
            }
            progress.message = progress.message.as_deref().map(safe_engine_message);
            if let Err(e) = self.persist_progress(&progress, tenant_id).await {
                self.cancel_and_fail(task_id, tenant_id, run_id, format!("持久化进度失败: {e}"))
                    .await;
                return;
            }
            let level = if progress.status == RunStatus::Failed {
                "ERROR"
            } else {
                "INFO"
            };
            let message = progress
                .message
                .as_deref()
                .map(safe_engine_message)
                .unwrap_or_else(|| {
                    format!("已接收真实引擎状态：{}", status_name(&progress.status))
                });
            self.record_log(task_id, tenant_id, run_id, level, &message)
                .await;
            match serde_json::to_string(&progress) {
                Ok(message) => {
                    let _ = self.ws_hub.tx.send((task_id.to_string(), message));
                },
                Err(e) => error!(run_id, error = %e, "progress serialization failed"),
            }
            if progress.status != RunStatus::Running {
                return;
            }
        }
        self.cancel_and_fail(
            task_id,
            tenant_id,
            run_id,
            "引擎进度通道在终态之前关闭".into(),
        )
        .await;
    }

    async fn cancel_and_fail(&self, task_id: &str, tenant_id: &str, run_id: &str, message: String) {
        if let Err(e) = self.engine.cancel(run_id).await {
            error!(run_id, error = %e, "cancel failed after progress error");
        }
        self.fail_run(task_id, tenant_id, run_id, message).await;
    }

    async fn fail_run(&self, task_id: &str, tenant_id: &str, run_id: &str, message: String) {
        self.record_log(
            task_id,
            tenant_id,
            run_id,
            "ERROR",
            "运行失败：详细诊断内容已保护，请检查受控服务日志",
        )
        .await;
        let progress = SyncProgress {
            run_id: run_id.into(),
            task_id: task_id.into(),
            status: RunStatus::Failed,
            rows_synced: 0,
            bytes_synced: 0,
            metrics_available: false,
            total_rows: None,
            current_table: None,
            percent: 0.0,
            message: Some(safe_engine_message(&message)),
        };
        if let Err(e) = self.persist_progress(&progress, tenant_id).await {
            error!(run_id, error = %e, "failed to persist failure state");
            if let Err(task_error) = require_update(
                self.task_repo
                    .update_status(task_id, tenant_id, "failed")
                    .await,
                "任务失败状态",
            ) {
                error!(task_id, error = %task_error, "failed to persist task failure state");
            }
        }
    }

    async fn record_log(
        &self,
        task_id: &str,
        tenant_id: &str,
        run_id: &str,
        level: &str,
        message: &str,
    ) {
        let scope = RunLogScope {
            tenant_id,
            task_id,
            run_id,
        };
        if let Err(error) = self
            .ws_hub
            .push_run_log(scope, level, "scheduler", message)
            .await
        {
            tracing::warn!(run_id, error = %error, "run log persistence failed");
        }
    }

    async fn persist_progress(
        &self,
        progress: &SyncProgress,
        tenant_id: &str,
    ) -> Result<(), BackendError> {
        let status = status_name(&progress.status);
        let rows_synced = checked_count(progress.rows_synced)?;
        let bytes_synced = checked_count(progress.bytes_synced)?;
        let total_rows = progress.total_rows.map(checked_count).transpose()?;
        require_update(
            self.task_run_repo
                .update(&UpdateTaskRunParams {
                    run_id: progress.run_id.clone(),
                    status: status.into(),
                    rows_synced,
                    bytes_synced,
                    total_rows,
                    total_bytes: None,
                    end_time: (progress.status != RunStatus::Running)
                        .then(|| chrono::Utc::now().to_rfc3339()),
                    error_message: (progress.status == RunStatus::Failed)
                        .then(|| progress.message.clone())
                        .flatten(),
                })
                .await,
            "PipelineRun 进度",
        )?;
        if progress.status != RunStatus::Running {
            require_update(
                self.task_repo
                    .update_status(&progress.task_id, tenant_id, status)
                    .await,
                "任务终态",
            )?;
        }
        Ok(())
    }

    pub fn has_active_runs(&self) -> bool {
        !self.active_runs.is_empty()
    }
    pub fn active_run_count(&self) -> usize {
        self.active_runs.len()
    }

    fn build_submission(
        &self,
        task: &dts_backend::types::TaskDetail,
        source: &dts_backend::repo::RawConnectionInfo,
        target: &dts_backend::repo::RawConnectionInfo,
        run_id: &str,
    ) -> Result<TaskSubmission, BackendError> {
        let config = PipelineConfig::parse(&task.config_json)?;
        Ok(TaskSubmission {
            run_id: run_id.into(),
            task_id: task.id.clone(),
            source: connection_config(source)?,
            target: connection_config(target)?,
            config: config.as_sync_config(),
        })
    }
}

fn connection_config(
    raw: &dts_backend::repo::RawConnectionInfo,
) -> Result<ConnectionConfig, BackendError> {
    let db_type = match raw.db_type.to_lowercase().as_str() {
        "mysql" => "mysql",
        "pg" | "postgresql" | "postgres" => "pg",
        "duckdb" => "duckdb",
        "swanlake" => "swanlake",
        _ => {
            return Err(BackendError::ConstraintViolation(
                "当前全量适配器仅支持 MySQL/PostgreSQL/DuckDB 数据源".into(),
            ));
        },
    };
    let port = u16::try_from(raw.port)
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| {
            BackendError::ConstraintViolation("数据源 port 必须在 1 到 65535 之间".into())
        })?;
    let username = if matches!(db_type, "duckdb" | "swanlake") {
        raw.username.clone().unwrap_or_default()
    } else {
        raw.username
            .clone()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BackendError::ConstraintViolation("数据源 username 不能为空".into()))?
    };
    let database = raw
        .database_name
        .clone()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BackendError::ConstraintViolation("数据源 database 不能为空".into()))?;
    if raw.host.trim().is_empty() {
        return Err(BackendError::ConstraintViolation(
            "数据源 host 不能为空".into(),
        ));
    }
    Ok(ConnectionConfig {
        db_type: db_type.into(),
        host: raw.host.clone(),
        port,
        username,
        password: raw.password.clone().unwrap_or_default(),
        database,
    })
}

fn checked_count(value: u64) -> Result<i64, BackendError> {
    i64::try_from(value).map_err(|_| BackendError::Internal("引擎计数超出持久化范围".into()))
}

fn require_update(result: Result<bool, BackendError>, context: &str) -> Result<(), BackendError> {
    if result? {
        Ok(())
    } else {
        Err(BackendError::NotFound(format!("{context}对应记录不存在")))
    }
}

fn status_name(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}
