use std::sync::Arc;

use dashmap::DashMap;
use dts_backend::BackendError;
use dts_backend::repo::{
    CreateTaskRunParams, DatasourceRepo, TaskRepo, TaskRunRepo, UpdateTaskRunParams,
};
use dts_engine::{ConnectionConfig, EngineService, TaskSubmission};
use tracing::{error, info};
use uuid::Uuid;

use crate::ws::task_log::WsHub;

#[cfg(test)]
mod tests;

/// 任务调度器 — 管理同步任务的启动、进度监听和状态更新
#[derive(Clone)]
pub struct TaskScheduler {
    engine: Arc<dyn EngineService>,
    task_repo: Arc<dyn TaskRepo>,
    task_run_repo: Arc<dyn TaskRunRepo>,
    datasource_repo: Arc<dyn DatasourceRepo>,
    ws_hub: Arc<WsHub>,
    active_runs: Arc<DashMap<String, ()>>,
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

    /// 启动同步任务
    ///
    /// 1. 查找任务详情和关联的数据源配置
    /// 2. 提交到引擎执行
    /// 3. 创建 task_run 记录
    /// 4. 更新任务状态为 "running"
    /// 5. 启动后台任务监听进度并通过 WebSocket 推送
    pub async fn start_task(&self, task_id: &str, tenant_id: &str) -> Result<String, BackendError> {
        // 1. 查找任务详情
        let task = self
            .task_repo
            .find_detail_by_id(task_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound(format!("任务不存在: {task_id}")))?;

        // 2. 查询真实数据源连接信息（含原始密码）
        let source_conn = self
            .datasource_repo
            .get_connection_info(&task.source_id, tenant_id)
            .await?
            .ok_or_else(|| {
                BackendError::NotFound(format!("源端数据源不存在: {}", task.source_id))
            })?;
        let target_conn = self
            .datasource_repo
            .get_connection_info(&task.target_id, tenant_id)
            .await?
            .ok_or_else(|| {
                BackendError::NotFound(format!("目标端数据源不存在: {}", task.target_id))
            })?;

        // 3. 构建提交参数（使用真实数据源配置）
        let submission = self.build_submission(&task, &source_conn, &target_conn)?;

        // 4. 提交到引擎
        // 每个任务独立创建一个 channel 对，任务结束即销毁，范围受限
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run_id = self
            .engine
            .submit(submission, tx)
            .await
            .map_err(|e| BackendError::Internal(format!("引擎提交失败: {e}")))?;

        // 5. 创建 task_run 记录
        let task_run_id = Uuid::new_v4().to_string();
        self.task_run_repo
            .create(&CreateTaskRunParams {
                id: task_run_id,
                task_id: task_id.to_string(),
                status: "running".to_string(),
            })
            .await?;

        // 6. 更新任务状态为 "running"
        self.task_repo
            .update_status(task_id, tenant_id, "running")
            .await?;

        info!(task_id = %task_id, run_id = %run_id, "同步任务已启动");

        // 6. 后台监听进度并推送/持久化
        let hub = self.ws_hub.clone();
        let run_repo = self.task_run_repo.clone();
        let task_repo = self.task_repo.clone();
        let tid = task_id.to_string();
        let tid2 = tenant_id.to_string();
        let rid = run_id.clone();
        let active = self.active_runs.clone();

        active.insert(rid.clone(), ());
        tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                // 推送进度到 WebSocket 客户端（通过 task_id channel）
                let msg = serde_json::to_string(&progress).unwrap_or_else(|e| {
                    tracing::error!(error = %e, "序列化 SyncProgress 失败");
                    r#"{}"#.to_string()
                });

                // 使用 WsHub 的 broadcast channel 发送给所有监听该 task 的客户端
                let _ = hub.tx.send((tid.clone(), msg));

                // 更新 task_run 记录
                let update_params = UpdateTaskRunParams {
                    run_id: rid.clone(),
                    status: format!("{:?}", progress.status).to_lowercase(),
                    rows_synced: progress.rows_synced as i64,
                    bytes_synced: progress.bytes_synced as i64,
                    total_rows: progress.total_rows.map(|v| v as i64),
                    total_bytes: None,
                    end_time: if progress.status != dts_engine::RunStatus::Running {
                        Some(chrono::Utc::now().to_rfc3339())
                    } else {
                        None
                    },
                    error_message: progress.message.clone(),
                };

                if let Err(e) = run_repo.update(&update_params).await {
                    error!(run_id = %rid, error = %e, "更新 task_run 记录失败");
                }

                // 任务结束时更新 task 状态
                match progress.status {
                    dts_engine::RunStatus::Completed => {
                        let _ = task_repo.update_status(&tid, &tid2, "completed").await;
                        info!(task_id = %tid, run_id = %rid, "同步任务完成");
                        active.remove(&rid);
                    },
                    dts_engine::RunStatus::Failed => {
                        let _ = task_repo.update_status(&tid, &tid2, "failed").await;
                        error!(task_id = %tid, run_id = %rid, "同步任务失败");
                        active.remove(&rid);
                    },
                    dts_engine::RunStatus::Cancelled => {
                        let _ = task_repo.update_status(&tid, &tid2, "failed").await;
                        info!(task_id = %tid, run_id = %rid, "同步任务已取消");
                        active.remove(&rid);
                    },
                    _ => {},
                }
            }
            active.remove(&rid);
        });

        Ok(run_id)
    }

    /// 判断是否有正在运行的任务
    pub fn has_active_runs(&self) -> bool {
        !self.active_runs.is_empty()
    }

    /// 获取活跃的 run 数量
    pub fn active_run_count(&self) -> usize {
        self.active_runs.len()
    }
}

// ============================================================================
// 辅助方法
// ============================================================================

impl TaskScheduler {
    /// 从 TaskDetail 和原始连接信息构建引擎提交参数
    fn build_submission(
        &self,
        task: &dts_backend::types::TaskDetail,
        source_conn: &dts_backend::repo::RawConnectionInfo,
        target_conn: &dts_backend::repo::RawConnectionInfo,
    ) -> Result<TaskSubmission, BackendError> {
        let config: TaskConfig =
            serde_json::from_str(&task.config_json).unwrap_or_else(|_| TaskConfig::default());

        let source_type = task.source_type.to_lowercase();
        let target_type = task.target_type.to_lowercase();

        Ok(TaskSubmission {
            task_id: task.id.clone(),
            source: ConnectionConfig {
                db_type: source_type,
                host: source_conn.host.clone(),
                port: source_conn.port as u16,
                username: source_conn.username.clone().unwrap_or_default(),
                password: source_conn.password.clone().unwrap_or_default(),
                database: source_conn.database_name.clone().unwrap_or_default(),
            },
            target: ConnectionConfig {
                db_type: target_type,
                host: target_conn.host.clone(),
                port: target_conn.port as u16,
                username: target_conn.username.clone().unwrap_or_default(),
                password: target_conn.password.clone().unwrap_or_default(),
                database: target_conn.database_name.clone().unwrap_or_default(),
            },
            config: dts_engine::SyncConfig {
                table_mappings: config
                    .table_mappings
                    .into_iter()
                    .map(|m| dts_engine::TableMapping {
                        source_table: m.source_table,
                        target_table: m.target_table,
                        source_schema: m.source_schema,
                        target_schema: m.target_schema,
                    })
                    .collect(),
                mode: match config.sync_mode.as_deref() {
                    Some("incremental") => dts_engine::SyncMode::Incremental,
                    Some("full_then_incremental") => dts_engine::SyncMode::FullThenIncremental,
                    _ => dts_engine::SyncMode::Full,
                },
                batch_size: config.batch_size.unwrap_or(1000),
            },
        })
    }
}

/// 从 task config_json 解析的任务配置（内部使用）
#[derive(Debug, Clone, serde::Deserialize)]
struct TaskConfig {
    #[serde(default)]
    table_mappings: Vec<TableMappingConfig>,
    #[serde(default)]
    sync_mode: Option<String>,
    #[serde(default)]
    batch_size: Option<usize>,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            table_mappings: vec![],
            sync_mode: None,
            batch_size: Some(1000),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TableMappingConfig {
    source_table: String,
    target_table: String,
    #[serde(default)]
    source_schema: Option<String>,
    #[serde(default)]
    target_schema: Option<String>,
}
