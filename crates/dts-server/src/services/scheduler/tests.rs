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
use std::sync::Mutex;

use async_trait::async_trait;
use dts_backend::repo::RawConnectionInfo;
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
// Mock DatasourceRepo
// ============================================================

struct MockDatasourceRepo {
    conn_info: Mutex<Option<RawConnectionInfo>>,
}

impl MockDatasourceRepo {
    fn new() -> Self {
        Self {
            conn_info: Mutex::new(None),
        }
    }

    fn set_connection_info(&self, info: Option<RawConnectionInfo>) {
        *self.conn_info.lock().unwrap() = info;
    }
}

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
        _: &str,
        _: &str,
    ) -> Result<Option<RawConnectionInfo>, BackendError> {
        Ok(self.conn_info.lock().unwrap().clone())
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
    let datasource_repo = Arc::new(MockDatasourceRepo::new());
    let ws_hub = Arc::new(WsHub::new());

    let scheduler = TaskScheduler::new(engine, task_repo, task_run_repo, datasource_repo, ws_hub);

    let result = scheduler.start_task("nonexistent", "tenant-1").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("不存在"),
        "expected 'not found' error, got: {err}"
    );
}

#[tokio::test]
async fn start_task_creates_run_and_updates_status() {
    let engine = Arc::new(MockEngine::new());
    let task_repo = Arc::new(MockTaskRepo::new());
    let task_run_repo = Arc::new(MockTaskRunRepo::new());
    let datasource_repo = Arc::new(MockDatasourceRepo::new());
    let ws_hub = Arc::new(WsHub::new());

    datasource_repo.set_connection_info(Some(RawConnectionInfo {
        db_type: "mysql".into(),
        host: "10.0.0.1".into(),
        port: 3306,
        username: Some("root".into()),
        password: Some("secret".into()),
        database_name: Some("source_db".into()),
    }));

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

    let scheduler = TaskScheduler::new(
        engine,
        task_repo.clone(),
        task_run_repo.clone(),
        datasource_repo,
        ws_hub,
    );

    let result = scheduler.start_task("task-1", "tenant-1").await;
    assert!(result.is_ok());
    let run_id = result.unwrap();
    assert!(!run_id.is_empty());

    let created = task_run_repo.created_runs();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].task_id, "task-1");
    assert_eq!(created[0].status, "running");

    let updates = task_repo.status_updates();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0],
        (
            "task-1".to_string(),
            "tenant-1".to_string(),
            "running".to_string()
        )
    );
}

#[tokio::test]
async fn scheduler_has_active_runs() {
    let engine = Arc::new(MockEngine::new());
    let task_repo = Arc::new(MockTaskRepo::new());
    let task_run_repo = Arc::new(MockTaskRunRepo::new());
    let datasource_repo = Arc::new(MockDatasourceRepo::new());
    let ws_hub = Arc::new(WsHub::new());

    datasource_repo.set_connection_info(Some(RawConnectionInfo {
        db_type: "mysql".into(),
        host: "localhost".into(),
        port: 3306,
        username: Some("root".into()),
        password: Some("pwd".into()),
        database_name: Some("db".into()),
    }));

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

    let scheduler = TaskScheduler::new(engine, task_repo, task_run_repo, datasource_repo, ws_hub);

    assert_eq!(scheduler.active_run_count(), 0);
    assert!(!scheduler.has_active_runs());

    let result = scheduler.start_task("task-2", "tenant-1").await;
    assert!(result.is_ok());
}
