use async_trait::async_trait;
use dts_engine::{EngineError, EngineService, SyncProgress, TaskSubmission};
use tracing::{error, info, warn};

use crate::adapter;

/// ape-dts 引擎服务
///
/// 当前为骨架实现。submit 通过 tokio::spawn 异步执行，
/// cancel 尚未实现（ape-dts 可能不支持运行时取消）。
#[derive(Clone)]
pub struct ApeEngineService;

impl ApeEngineService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ApeEngineService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EngineService for ApeEngineService {
    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let task_id = task.task_id.clone();
        let source_type = task.source.db_type.clone();
        let target_type = task.target.db_type.clone();

        info!(
            run_id = %run_id,
            task_id = %task_id,
            source_type = %source_type,
            target_type = %target_type,
            "提交同步任务到 ape-dts 引擎"
        );

        let rid = run_id.clone();
        tokio::spawn(async move {
            if let Err(e) = adapter::run_sync(task, &rid, progress_tx).await {
                error!(run_id = %rid, error = %e, "同步任务执行失败");
            }
        });

        Ok(run_id)
    }

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError> {
        warn!(run_id = %run_id, "cancel 尚未实现（ape-dts 不支持运行时取消）");
        Err(EngineError::Internal(
            "cancel 尚未实现：ape-dts 暂不支持运行时取消".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dts_engine::{ConnectionConfig, RunStatus, SyncConfig, SyncMode, TableMapping};

    fn make_test_submission() -> TaskSubmission {
        TaskSubmission {
            task_id: "task-test-1".into(),
            source: ConnectionConfig {
                db_type: "mysql".into(),
                host: "localhost".into(),
                port: 3306,
                username: "root".into(),
                password: "secret".into(),
                database: "source_db".into(),
            },
            target: ConnectionConfig {
                db_type: "postgresql".into(),
                host: "localhost".into(),
                port: 5432,
                username: "postgres".into(),
                password: "secret".into(),
                database: "target_db".into(),
            },
            config: SyncConfig {
                table_mappings: vec![TableMapping {
                    source_table: "users".into(),
                    target_table: "users".into(),
                    source_schema: None,
                    target_schema: Some("public".into()),
                }],
                mode: SyncMode::Full,
                batch_size: 1000,
            },
        }
    }

    #[tokio::test]
    async fn submit_returns_run_id() {
        let engine = ApeEngineService::new();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let result = engine.submit(make_test_submission(), tx).await;
        assert!(result.is_ok());
        let run_id = result.unwrap();
        assert_eq!(run_id.len(), 36);
        assert_eq!(run_id.chars().filter(|c| *c == '-').count(), 4);
    }

    #[tokio::test]
    async fn submit_starts_background_task() {
        let engine = ApeEngineService::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let result = engine.submit(make_test_submission(), tx).await;
        assert!(result.is_ok());

        // 等待背景任务发送进度
        let progress = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("should receive a progress message");

        assert_eq!(progress.task_id, "task-test-1");
        assert!(matches!(progress.status, RunStatus::Running));
        assert!(progress.message.as_deref().unwrap().contains("ape-dts"));
    }

    #[tokio::test]
    async fn cancel_returns_error_not_implemented() {
        let engine = ApeEngineService::new();
        let result = engine.cancel("run-123").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("尚未实现"));
    }
}
