use std::sync::Arc;

use dashmap::DashMap;
use dts_backend::repo::{CreateTaskRunParams, TaskRepo, TaskRunRepo, UpdateTaskRunParams};
use dts_backend::BackendError;
use dts_engine::{EngineService, TaskSubmission};
use tracing::{error, info};
use uuid::Uuid;

use crate::ws::task_log::WsHub;

/// 任务调度器 — 管理同步任务的启动、进度监听和状态更新
#[derive(Clone)]
pub struct TaskScheduler {
    engine: Arc<dyn EngineService>,
    task_repo: Arc<dyn TaskRepo>,
    task_run_repo: Arc<dyn TaskRunRepo>,
    ws_hub: Arc<WsHub>,
    active_runs: Arc<DashMap<String, ()>>,
}

impl TaskScheduler {
    pub fn new(
        engine: Arc<dyn EngineService>,
        task_repo: Arc<dyn TaskRepo>,
        task_run_repo: Arc<dyn TaskRunRepo>,
        ws_hub: Arc<WsHub>,
    ) -> Self {
        Self {
            engine,
            task_repo,
            task_run_repo,
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

        // 2. 构建提交参数（骨架实现：使用任务配置中的信息）
        let submission = self.build_submission(&task)?;

        // 3. 提交到引擎
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run_id = self
            .engine
            .submit(submission, tx)
            .await
            .map_err(|e| BackendError::Internal(format!("引擎提交失败: {e}")))?;

        // 4. 创建 task_run 记录
        let task_run_id = Uuid::new_v4().to_string();
        self.task_run_repo
            .create(&CreateTaskRunParams {
                id: task_run_id,
                task_id: task_id.to_string(),
                status: "running".to_string(),
            })
            .await?;

        // 5. 更新任务状态为 "running"
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
                let msg =
                    serde_json::to_string(&progress).unwrap_or_else(|_| r#"{}"#.to_string());

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
                    }
                    dts_engine::RunStatus::Failed => {
                        let _ = task_repo.update_status(&tid, &tid2, "failed").await;
                        error!(task_id = %tid, run_id = %rid, "同步任务失败");
                        active.remove(&rid);
                    }
                    dts_engine::RunStatus::Cancelled => {
                        let _ = task_repo.update_status(&tid, &tid2, "failed").await;
                        info!(task_id = %tid, run_id = %rid, "同步任务已取消");
                        active.remove(&rid);
                    }
                    _ => {}
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
    /// 从 TaskDetail 构建引擎提交参数
    fn build_submission(
        &self,
        task: &dts_backend::types::TaskDetail,
    ) -> Result<TaskSubmission, BackendError> {
        // 解析 config_json 中的表映射
        let config: TaskConfig = serde_json::from_str(&task.config_json)
            .unwrap_or_else(|_| TaskConfig::default());

        Ok(TaskSubmission {
            task_id: task.id.clone(),
            source: dts_engine::ConnectionConfig {
                db_type: task.source_type.clone(),
                host: "localhost".to_string(), // TODO: 从 task source datasource 获取
                port: 3306,                    // TODO: 从 datasource 获取
                username: "".to_string(),      // TODO: 从 datasource 获取
                password: "".to_string(),      // TODO: 解密
                database: "".to_string(),      // TODO: 从 datasource 获取
            },
            target: dts_engine::ConnectionConfig {
                db_type: task.target_type.clone(),
                host: "localhost".to_string(), // TODO: 同上
                port: 5432,                    // TODO: 同上
                username: "".to_string(),      // TODO: 同上
                password: "".to_string(),      // TODO: 同上
                database: "".to_string(),      // TODO: 同上
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::repo::task_run::CreateTaskRunParams as TRCreateParams;
    use dts_backend::repo::task_run::UpdateTaskRunParams as TRUpdateParams;
    use dts_backend::types::*;
    use dts_engine::SyncProgress;

    // ============================================================
    // Mock EngineService
    // ============================================================

    struct MockEngine {
        submit_result: Mutex<Option<Result<String, dts_engine::EngineError>>>,
    }

    impl MockEngine {
        fn new() -> Self {
            Self {
                submit_result: Mutex::new(None),
            }
        }

        fn set_submit_result(&self, result: Result<String, dts_engine::EngineError>) {
            *self.submit_result.lock().unwrap() = Some(result);
        }
    }

    #[async_trait]
    impl EngineService for MockEngine {
        async fn submit(
            &self,
            _task: TaskSubmission,
            _progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
        ) -> Result<String, dts_engine::EngineError> {
            self.submit_result
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(Ok("mock-run-id".into()))
        }

        async fn cancel(&self, _run_id: &str) -> Result<(), dts_engine::EngineError> {
            Ok(())
        }
    }

    // ============================================================
    // Mock TaskRepo
    // ============================================================

    struct MockTaskRepo {
        detail_result: Mutex<Option<Option<TaskDetail>>>,
        status_updates: Mutex<Vec<(String, String, String)>>,
    }

    impl MockTaskRepo {
        fn new() -> Self {
            Self {
                detail_result: Mutex::new(None),
                status_updates: Mutex::new(vec![]),
            }
        }

        fn set_detail(&self, detail: Option<TaskDetail>) {
            *self.detail_result.lock().unwrap() = Some(detail);
        }

        fn status_updates(&self) -> Vec<(String, String, String)> {
            self.status_updates.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TaskRepo for MockTaskRepo {
        async fn create(&self, _: &CreateTaskParams) -> Result<String, BackendError> {
            Ok("ignored".into())
        }
        async fn list_by_tenant(&self, _: &str) -> Result<Vec<TaskInfo>, BackendError> {
            Ok(vec![])
        }
        async fn find_detail_by_id(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<TaskDetail>, BackendError> {
            Ok(self.detail_result.lock().unwrap().clone().unwrap_or(None))
        }
        async fn update_status(&self, id: &str, tid: &str, status: &str) -> Result<bool, BackendError> {
            self.status_updates
                .lock()
                .unwrap()
                .push((id.into(), tid.into(), status.into()));
            Ok(true)
        }
        async fn count_by_tenant(&self, _: &str) -> Result<i64, BackendError> {
            Ok(0)
        }
        async fn count_by_status(&self, _: &str, _: &str) -> Result<i64, BackendError> {
            Ok(0)
        }
        async fn find_recent_runs(&self, _: &str, _: i64) -> Result<Vec<TaskRunInfo>, BackendError> {
            Ok(vec![])
        }
    }

    // ============================================================
    // Mock TaskRunRepo
    // ============================================================

    struct MockTaskRunRepo {
        created: Mutex<Vec<TRCreateParams>>,
    }

    impl MockTaskRunRepo {
        fn new() -> Self {
            Self {
                created: Mutex::new(vec![]),
            }
        }

        fn created_runs(&self) -> Vec<TRCreateParams> {
            self.created.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TaskRunRepo for MockTaskRunRepo {
        async fn create(&self, params: &TRCreateParams) -> Result<String, BackendError> {
            self.created.lock().unwrap().push(params.clone());
            Ok(params.id.clone())
        }
        async fn update(&self, _: &TRUpdateParams) -> Result<bool, BackendError> {
            Ok(true)
        }
        async fn find_recent(&self, _: &str, _: i64) -> Result<Vec<TaskRunInfo>, BackendError> {
            Ok(vec![])
        }
    }

    // ============================================================
    // Tests
    // ============================================================

    #[tokio::test]
    async fn start_task_not_found_returns_error() {
        let engine = Arc::new(MockEngine::new());
        let task_repo = Arc::new(MockTaskRepo::new());
        let task_run_repo = Arc::new(MockTaskRunRepo::new());
        let ws_hub = Arc::new(WsHub::new());

        let scheduler = TaskScheduler::new(engine, task_repo, task_run_repo, ws_hub);

        let result = scheduler.start_task("nonexistent", "tenant-1").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("不存在"), "expected 'not found' error, got: {err}");
    }

    #[tokio::test]
    async fn start_task_creates_run_and_updates_status() {
        let engine = Arc::new(MockEngine::new());
        let task_repo = Arc::new(MockTaskRepo::new());
        let task_run_repo = Arc::new(MockTaskRunRepo::new());
        let ws_hub = Arc::new(WsHub::new());

        task_repo.set_detail(Some(TaskDetail {
            id: "task-1".into(),
            name: "Test Task".into(),
            description: None,
            source_id: "ds-1".into(),
            target_id: "ds-2".into(),
            source_name: "MySQL".into(),
            source_type: "mysql".into(),
            target_name: "PG".into(),
            target_type: "postgresql".into(),
            cron_expr: None,
            status: "draft".into(),
            config_json: r#"{"table_mappings":[{"source_table":"users","target_table":"users"}],"sync_mode":"full","batch_size":500}"#.into(),
            created_by: "admin".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            recent_runs: vec![],
        }));

        let scheduler = TaskScheduler::new(engine, task_repo.clone(), task_run_repo.clone(), ws_hub);

        let result = scheduler.start_task("task-1", "tenant-1").await;
        assert!(result.is_ok());
        let run_id = result.unwrap();
        assert!(!run_id.is_empty());

        // 验证 task_run 记录已创建
        let created = task_run_repo.created_runs();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].task_id, "task-1");
        assert_eq!(created[0].status, "running");

        // 验证任务状态已更新为 "running"
        let updates = task_repo.status_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0], ("task-1".to_string(), "tenant-1".to_string(), "running".to_string()));
    }

    #[tokio::test]
    async fn scheduler_has_active_runs() {
        let engine = Arc::new(MockEngine::new());
        let task_repo = Arc::new(MockTaskRepo::new());
        let task_run_repo = Arc::new(MockTaskRunRepo::new());
        let ws_hub = Arc::new(WsHub::new());

        task_repo.set_detail(Some(TaskDetail {
            id: "task-2".into(),
            name: "T2".into(),
            description: None,
            source_id: "s".into(),
            target_id: "t".into(),
            source_name: "S".into(),
            source_type: "mysql".into(),
            target_name: "T".into(),
            target_type: "pg".into(),
            cron_expr: None,
            status: "draft".into(),
            config_json: "{}".into(),
            created_by: "admin".into(),
            created_at: "".into(),
            updated_at: "".into(),
            recent_runs: vec![],
        }));

        let scheduler = TaskScheduler::new(engine, task_repo, task_run_repo, ws_hub);

        assert_eq!(scheduler.active_run_count(), 0);
        assert!(!scheduler.has_active_runs());

        let result = scheduler.start_task("task-2", "tenant-1").await;
        assert!(result.is_ok());
    }
}
