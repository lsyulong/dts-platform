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

use super::*;
use async_trait::async_trait;
use dts_backend::{repo::RawConnectionInfo, types::*};
use std::sync::Mutex;

type ProgressSender = tokio::sync::mpsc::UnboundedSender<SyncProgress>;

struct MockEngine {
    runs: Arc<MockTaskRunRepo>,
    submitted: Mutex<Vec<TaskSubmission>>,
    sender: Mutex<Option<ProgressSender>>,
    submit_fails: Mutex<bool>,
    cancelled: Mutex<Vec<String>>,
}

#[async_trait]
impl EngineService for MockEngine {
    fn supports(&self, _: &TaskSubmission) -> bool {
        true
    }

    async fn submit(
        &self,
        task: TaskSubmission,
        tx: ProgressSender,
    ) -> Result<String, dts_engine::EngineError> {
        assert!(
            self.runs
                .created
                .lock()
                .unwrap()
                .iter()
                .any(|run| run.id == task.run_id),
            "run must be persisted before execution is visible"
        );
        self.submitted.lock().unwrap().push(task.clone());
        if *self.submit_fails.lock().unwrap() {
            return Err(dts_engine::EngineError::Internal(
                "injected submit failure".into(),
            ));
        }
        *self.sender.lock().unwrap() = Some(tx);
        Ok(task.run_id)
    }
    async fn cancel(&self, run_id: &str) -> Result<(), dts_engine::EngineError> {
        self.cancelled.lock().unwrap().push(run_id.into());
        if let Some(tx) = self.sender.lock().unwrap().take() {
            let mut event = progress(run_id, RunStatus::Cancelled);
            event.message = Some("cancelled".into());
            let _ = tx.send(event);
        }
        Ok(())
    }
}

struct MockDatasourceRepo;
#[async_trait]
impl DatasourceRepo for MockDatasourceRepo {
    async fn create(&self, _: &CreateDatasourceParams) -> Result<String, BackendError> {
        Ok("ignored".into())
    }
    async fn list_by_tenant(&self, _: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
        Ok(vec![])
    }
    async fn find_detail_by_id(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<DatasourceDetail>, BackendError> {
        Ok(None)
    }
    async fn get_connection_info(
        &self,
        id: &str,
        _: &str,
    ) -> Result<Option<RawConnectionInfo>, BackendError> {
        Ok(Some(RawConnectionInfo {
            db_type: if id == "target" {
                "postgresql"
            } else {
                "mysql"
            }
            .into(),
            host: "localhost".into(),
            port: if id == "target" { 5432 } else { 3306 },
            username: Some("test".into()),
            password: Some("test".into()),
            database_name: Some(id.into()),
        }))
    }
    async fn delete(&self, _: &str, _: &str) -> Result<bool, BackendError> {
        Ok(true)
    }
    async fn count_by_tenant(&self, _: &str) -> Result<i64, BackendError> {
        Ok(0)
    }
    async fn count_healthy_by_tenant(&self, _: &str) -> Result<i64, BackendError> {
        Ok(0)
    }
    async fn exists(&self, _: &str, _: &str) -> Result<bool, BackendError> {
        Ok(true)
    }
}

struct MockTaskRepo {
    detail: Mutex<Option<TaskDetail>>,
    statuses: Mutex<Vec<String>>,
    update_succeeds: Mutex<bool>,
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
        Ok(self.detail.lock().unwrap().clone())
    }
    async fn update_status(&self, _: &str, _: &str, status: &str) -> Result<bool, BackendError> {
        self.statuses.lock().unwrap().push(status.into());
        Ok(*self.update_succeeds.lock().unwrap())
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

struct MockTaskRunRepo {
    created: Mutex<Vec<CreateTaskRunParams>>,
    updated: Mutex<Vec<UpdateTaskRunParams>>,
    create_fails: Mutex<bool>,
    update_succeeds: Mutex<bool>,
}
#[async_trait]
impl TaskRunRepo for MockTaskRunRepo {
    async fn create(&self, params: &CreateTaskRunParams) -> Result<String, BackendError> {
        if *self.create_fails.lock().unwrap() {
            return Err(BackendError::Query("injected create failure".into()));
        }
        self.created.lock().unwrap().push(params.clone());
        Ok(params.id.clone())
    }
    async fn update(&self, params: &UpdateTaskRunParams) -> Result<bool, BackendError> {
        self.updated.lock().unwrap().push(params.clone());
        Ok(*self.update_succeeds.lock().unwrap())
    }
    async fn find_recent(&self, _: &str, _: i64) -> Result<Vec<TaskRunInfo>, BackendError> {
        Ok(vec![])
    }
}

struct Harness {
    scheduler: TaskScheduler,
    engine: Arc<MockEngine>,
    tasks: Arc<MockTaskRepo>,
    runs: Arc<MockTaskRunRepo>,
}
impl Harness {
    fn new() -> Self {
        let runs = Arc::new(MockTaskRunRepo {
            created: Mutex::new(vec![]),
            updated: Mutex::new(vec![]),
            create_fails: Mutex::new(false),
            update_succeeds: Mutex::new(true),
        });
        let engine = Arc::new(MockEngine {
            runs: runs.clone(),
            submitted: Mutex::new(vec![]),
            sender: Mutex::new(None),
            submit_fails: Mutex::new(false),
            cancelled: Mutex::new(vec![]),
        });
        let tasks = Arc::new(MockTaskRepo {
            detail: Mutex::new(Some(task())),
            statuses: Mutex::new(vec![]),
            update_succeeds: Mutex::new(true),
        });
        let scheduler = TaskScheduler::new(
            engine.clone(),
            tasks.clone(),
            runs.clone(),
            Arc::new(MockDatasourceRepo),
            Arc::new(WsHub::new()),
        );
        Self {
            scheduler,
            engine,
            tasks,
            runs,
        }
    }
    async fn wait_finished(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while self.scheduler.has_active_runs() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

fn task() -> TaskDetail {
    TaskDetail { id: "task-1".into(), name: "Test".into(), description: None,
        source_id: "source".into(), target_id: "target".into(), source_name: "source".into(), source_type: "mysql".into(),
        target_name: "target".into(), target_type: "postgresql".into(), cron_expr: None, status: "draft".into(),
        config_json: r#"{"table_mappings":[{"source_table":"orders","target_table":"fact_orders","target_schema":"mart"}],"sync_mode":"full","batch_size":500}"#.into(),
        created_by: "admin".into(), created_at: "".into(), updated_at: "".into(), recent_runs: vec![] }
}
fn progress(run_id: &str, status: RunStatus) -> SyncProgress {
    SyncProgress {
        run_id: run_id.into(),
        task_id: "task-1".into(),
        status,
        rows_synced: 42,
        bytes_synced: 128,
        metrics_available: true,
        total_rows: Some(42),
        current_table: None,
        percent: 100.0,
        message: Some("progress message".into()),
    }
}

#[tokio::test]
async fn start_task_creates_run_and_updates_status() {
    let harness = Harness::new();
    let run_id = harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    assert_eq!(
        harness.runs.created.lock().unwrap()[0].id,
        run_id,
        "persisted PipelineRun ID must match engine progress ID"
    );
    let submitted = harness.engine.submitted.lock().unwrap();
    assert_eq!(submitted[0].run_id, run_id);
    assert_eq!(
        submitted[0].config.table_mappings[0]
            .target_schema
            .as_deref(),
        Some("mart")
    );
    assert_eq!(submitted[0].target.db_type, "pg");
    assert_eq!(
        harness.tasks.statuses.lock().unwrap().as_slice(),
        ["running"]
    );
}

#[tokio::test]
async fn completed_progress_updates_persisted_run_before_closing_registry() {
    let harness = Harness::new();
    let run_id = harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    harness
        .engine
        .sender
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(progress(&run_id, RunStatus::Completed))
        .unwrap();
    harness.wait_finished().await;
    let updates = harness.runs.updated.lock().unwrap();
    assert_eq!(updates[0].run_id, run_id);
    assert_eq!(updates[0].status, "completed");
    assert_eq!(updates[0].rows_synced, 42);
    assert!(updates[0].error_message.is_none());
    assert!(updates[0].end_time.is_some());
    assert_eq!(
        harness.tasks.statuses.lock().unwrap().last().unwrap(),
        "completed"
    );
}

#[tokio::test]
async fn failed_run_record_creation_never_submits_external_execution() {
    let harness = Harness::new();
    *harness.runs.create_fails.lock().unwrap() = true;
    assert!(
        harness
            .scheduler
            .start_task("task-1", "tenant-1")
            .await
            .is_err()
    );
    assert!(harness.engine.submitted.lock().unwrap().is_empty());
    assert!(!harness.scheduler.has_active_runs());
}

#[tokio::test]
async fn submit_failure_persists_failed_run_instead_of_leaving_running() {
    let harness = Harness::new();
    *harness.engine.submit_fails.lock().unwrap() = true;
    assert!(
        harness
            .scheduler
            .start_task("task-1", "tenant-1")
            .await
            .is_err()
    );
    let updates = harness.runs.updated.lock().unwrap();
    assert_eq!(
        updates[0].run_id,
        harness.runs.created.lock().unwrap()[0].id
    );
    assert_eq!(updates[0].status, "failed");
    assert!(updates[0].end_time.is_some());
    assert!(!harness.scheduler.has_active_runs());
}

#[tokio::test]
async fn missing_task_status_update_prevents_submission_and_finishes_run() {
    let harness = Harness::new();
    *harness.tasks.update_succeeds.lock().unwrap() = false;
    assert!(
        harness
            .scheduler
            .start_task("task-1", "tenant-1")
            .await
            .is_err()
    );
    assert!(harness.engine.submitted.lock().unwrap().is_empty());
    assert_eq!(harness.runs.updated.lock().unwrap()[0].status, "failed");
}

#[tokio::test]
async fn concurrent_start_of_same_pipeline_is_rejected() {
    let harness = Harness::new();
    let (first, second) = tokio::join!(
        harness.scheduler.start_task("task-1", "tenant-1"),
        harness.scheduler.start_task("task-1", "tenant-1")
    );
    assert_ne!(first.is_ok(), second.is_ok());
    assert_eq!(harness.engine.submitted.lock().unwrap().len(), 1);
    assert_eq!(harness.runs.created.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn unexpected_progress_channel_close_finishes_as_failed() {
    let harness = Harness::new();
    harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    harness.engine.sender.lock().unwrap().take();
    harness.wait_finished().await;
    assert_eq!(
        harness.runs.updated.lock().unwrap().last().unwrap().status,
        "failed"
    );
    assert_eq!(
        harness.tasks.statuses.lock().unwrap().last().unwrap(),
        "failed"
    );
}

#[tokio::test]
async fn failed_persistence_of_progress_cancels_execution() {
    let harness = Harness::new();
    let run_id = harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    *harness.runs.update_succeeds.lock().unwrap() = false;
    harness
        .engine
        .sender
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(progress(&run_id, RunStatus::Running))
        .unwrap();
    harness.wait_finished().await;
    assert_eq!(
        harness.engine.cancelled.lock().unwrap().as_slice(),
        [run_id]
    );
    assert_eq!(
        harness.tasks.statuses.lock().unwrap().last().unwrap(),
        "failed"
    );
}

#[tokio::test]
async fn cancellation_is_reported_as_cancelled_after_engine_confirmation() {
    let harness = Harness::new();
    let run_id = harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    harness
        .scheduler
        .cancel_task("task-1", "tenant-1")
        .await
        .unwrap();
    harness.wait_finished().await;
    assert_eq!(
        harness.engine.cancelled.lock().unwrap().as_slice(),
        [run_id]
    );
    assert_eq!(
        harness.runs.updated.lock().unwrap().last().unwrap().status,
        "cancelled"
    );
    assert_eq!(
        harness.tasks.statuses.lock().unwrap().last().unwrap(),
        "cancelled"
    );
}

#[tokio::test]
async fn invalid_config_does_not_submit_a_default_empty_snapshot() {
    for invalid in [
        "broken json",
        "{}",
        r#"{"table_mappings":{"orders":"fact_orders"}}"#,
    ] {
        let harness = Harness::new();
        harness
            .tasks
            .detail
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .config_json = invalid.into();
        assert!(
            harness
                .scheduler
                .start_task("task-1", "tenant-1")
                .await
                .is_err()
        );
        assert!(harness.engine.submitted.lock().unwrap().is_empty());
        assert!(harness.runs.created.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn wrong_progress_id_does_not_update_another_run() {
    let harness = Harness::new();
    let run_id = harness
        .scheduler
        .start_task("task-1", "tenant-1")
        .await
        .unwrap();
    harness
        .engine
        .sender
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(progress("wrong-run", RunStatus::Completed))
        .unwrap();
    harness.wait_finished().await;
    let updates = harness.runs.updated.lock().unwrap();
    assert!(updates.iter().all(|update| update.run_id == run_id));
    assert_eq!(updates.last().unwrap().status, "failed");
}
