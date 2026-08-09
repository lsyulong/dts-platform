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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dts_engine::{EngineError, EngineService, SyncProgress, TaskSubmission};
use tokio::sync::oneshot;
use tracing::{error, info, warn};

use crate::adapter;

type CancelMap = Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>;

/// ape-dts 引擎服务
///
/// `submit` 通过 tokio::spawn 异步执行 ape-dts CLI 子进程。
/// `cancel` 通过 oneshot channel 通知子进程退出，
/// 适配器收到信号后 kill 子进程并清理临时文件。
#[derive(Clone)]
pub struct ApeEngineService {
    cancel_txs: CancelMap,
}

impl ApeEngineService {
    pub fn new() -> Self {
        Self {
            cancel_txs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 活跃任务数（用于监控/测试）
    pub fn active_count(&self) -> usize {
        self.cancel_txs.lock().unwrap_or_else(|e| e.into_inner()).len()
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

        // 创建取消通道
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        {
            let mut map = self.cancel_txs.lock().unwrap_or_else(|e| e.into_inner());
            map.insert(run_id.clone(), cancel_tx);
        }

        let rid = run_id.clone();
        let cancel_map = self.cancel_txs.clone();
        tokio::spawn(async move {
            if let Err(e) = adapter::run_sync(task, &rid, progress_tx, cancel_rx).await {
                error!(run_id = %rid, error = %e, "同步任务执行失败");
            }
            // 清理取消通道
            let mut map = cancel_map.lock().unwrap_or_else(|e| e.into_inner());
            map.remove(&rid);
        });

        Ok(run_id)
    }

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError> {
        let tx = {
            let mut map = self.cancel_txs.lock().unwrap_or_else(|e| e.into_inner());
            map.remove(run_id)
        };

        match tx {
            Some(tx) => {
                info!(run_id = %run_id, "发送取消信号到 ape-dts 子进程");
                // oneshot::send 可能失败（接收端已 dropped），但这种情况说明任务已结束
                if tx.send(()).is_err() {
                    warn!(run_id = %run_id, "取消信号发送失败：任务可能已结束");
                    Ok(())
                } else {
                    Ok(())
                }
            },
            None => {
                // 任务 ID 不存在或已结束
                Err(EngineError::Internal(format!(
                    "无法取消：run_id '{run_id}' 不存在或已结束"
                )))
            },
        }
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
    async fn submit_sends_progress() {
        let engine = ApeEngineService::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let result = engine.submit(make_test_submission(), tx).await;
        assert!(result.is_ok());

        // 等待背景任务发送进度（启动消息总是立即发送）
        let progress = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout")
            .expect("should receive a progress message");

        assert_eq!(progress.task_id, "task-test-1");
        assert!(matches!(progress.status, RunStatus::Running));
    }

    #[tokio::test]
    async fn cancel_nonexistent_run_returns_error() {
        let engine = ApeEngineService::new();
        let result = engine.cancel("nonexistent-run").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("不存在或已结束"));
    }

    #[tokio::test]
    async fn cancel_cleans_up_registry() {
        let engine = ApeEngineService::new();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let run_id = engine
            .submit(make_test_submission(), tx)
            .await
            .expect("submit failed");

        assert_eq!(engine.active_count(), 1);

        // Cancel should succeed
        let result = engine.cancel(&run_id).await;
        assert!(result.is_ok());

        // Second cancel should fail (already removed)
        let result2 = engine.cancel(&run_id).await;
        assert!(result2.is_err());
    }
}
