use std::sync::Arc;

use anyhow::anyhow;
use dts_backend::repo::{DatasourceRepo, TaskRepo};
use dts_backend::types::*;
use uuid::Uuid;

/// 同步任务管理服务
#[derive(Clone)]
pub struct TaskService {
    task_repo: Arc<dyn TaskRepo>,
    datasource_repo: Arc<dyn DatasourceRepo>,
}

impl TaskService {
    pub fn new(task_repo: Arc<dyn TaskRepo>, datasource_repo: Arc<dyn DatasourceRepo>) -> Self {
        Self {
            task_repo,
            datasource_repo,
        }
    }

    /// 创建同步任务 — 校验源和目标数据源存在后创建
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        description: Option<&str>,
        source_id: &str,
        target_id: &str,
        cron_expr: Option<&str>,
        config_json: &str,
        created_by: &str,
    ) -> anyhow::Result<String> {
        // 校验源数据源存在
        let source_exists = self.datasource_repo.exists(source_id, tenant_id).await?;
        if !source_exists {
            return Err(anyhow!("源数据源 '{source_id}' 不存在或不属于该租户"));
        }

        // 校验目标数据源存在
        let target_exists = self.datasource_repo.exists(target_id, tenant_id).await?;
        if !target_exists {
            return Err(anyhow!("目标数据源 '{target_id}' 不存在或不属于该租户"));
        }

        let id = Uuid::new_v4().to_string();

        let params = CreateTaskParams {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            cron_expr: cron_expr.map(|s| s.to_string()),
            config_json: config_json.to_string(),
            created_by: created_by.to_string(),
        };

        self.task_repo.create(&params).await?;

        tracing::info!("同步任务 '{name}' 创建成功");

        Ok(id)
    }

    /// 获取租户下的所有同步任务
    pub async fn list_by_tenant(&self, tenant_id: &str) -> anyhow::Result<Vec<TaskInfo>> {
        Ok(self.task_repo.list_by_tenant(tenant_id).await?)
    }

    /// 更新任务状态 — 校验状态转换合法性
    ///
    /// 合法转换:
    /// - draft → running
    /// - running → paused
    /// - running → completed
    /// - running → failed
    pub async fn update_status(
        &self,
        id: &str,
        tenant_id: &str,
        new_status: &str,
    ) -> anyhow::Result<bool> {
        let task = self
            .task_repo
            .find_detail_by_id(id, tenant_id)
            .await?
            .ok_or_else(|| anyhow!("任务 '{id}' 不存在"))?;

        let current = task.status.as_str();

        let valid = matches!(
            (current, new_status),
            ("draft", "running")
                | ("running", "paused")
                | ("running", "completed")
                | ("running", "failed")
        );

        if !valid {
            return Err(anyhow!("无效的状态转换: {current} → {new_status}"));
        }

        Ok(self
            .task_repo
            .update_status(id, tenant_id, new_status)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::BackendError;

    // ============================================================
    // Mock TaskRepo
    // ============================================================

    struct MockTaskRepo {
        last_create_params: Mutex<Option<CreateTaskParams>>,
        list_result: Mutex<Vec<TaskInfo>>,
        find_detail_result: Mutex<Option<TaskDetail>>,
        update_status_calls: Mutex<Vec<(String, String, String)>>,
        update_status_result: Mutex<bool>,
    }

    impl MockTaskRepo {
        fn new() -> Self {
            Self {
                last_create_params: Mutex::new(None),
                list_result: Mutex::new(vec![]),
                find_detail_result: Mutex::new(None),
                update_status_calls: Mutex::new(vec![]),
                update_status_result: Mutex::new(true),
            }
        }

        fn set_list_result(&self, tasks: Vec<TaskInfo>) {
            *self.list_result.lock().unwrap() = tasks;
        }

        fn set_find_detail_result(&self, detail: Option<TaskDetail>) {
            *self.find_detail_result.lock().unwrap() = detail;
        }

        fn last_create_params(&self) -> Option<CreateTaskParams> {
            self.last_create_params.lock().unwrap().clone()
        }

        fn update_status_calls(&self) -> Vec<(String, String, String)> {
            self.update_status_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TaskRepo for MockTaskRepo {
        async fn create(&self, params: &CreateTaskParams) -> Result<String, BackendError> {
            *self.last_create_params.lock().unwrap() = Some(params.clone());
            Ok(params.id.clone())
        }

        async fn list_by_tenant(&self, _tenant_id: &str) -> Result<Vec<TaskInfo>, BackendError> {
            Ok(self.list_result.lock().unwrap().clone())
        }

        async fn find_detail_by_id(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<TaskDetail>, BackendError> {
            Ok(self.find_detail_result.lock().unwrap().clone())
        }

        async fn update_status(
            &self,
            id: &str,
            tenant_id: &str,
            status: &str,
        ) -> Result<bool, BackendError> {
            self.update_status_calls.lock().unwrap().push((
                id.to_string(),
                tenant_id.to_string(),
                status.to_string(),
            ));
            Ok(*self.update_status_result.lock().unwrap())
        }

        async fn count_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn count_by_status(
            &self,
            _tenant_id: &str,
            _status: &str,
        ) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn find_recent_runs(
            &self,
            _task_id: &str,
            _limit: i64,
        ) -> Result<Vec<TaskRunInfo>, BackendError> {
            Ok(vec![])
        }
    }

    // ============================================================
    // Mock DatasourceRepo (for TaskService tests)
    // ============================================================

    struct MockDatasourceRepo {
        existing_ids: Mutex<Vec<String>>,
    }

    impl MockDatasourceRepo {
        fn new() -> Self {
            Self {
                existing_ids: Mutex::new(vec![]),
            }
        }

        fn add_existing(&self, id: &str) {
            self.existing_ids.lock().unwrap().push(id.to_string());
        }
    }

    #[async_trait]
    impl DatasourceRepo for MockDatasourceRepo {
        async fn create(&self, _params: &CreateDatasourceParams) -> Result<String, BackendError> {
            Ok("ignored".to_string())
        }

        async fn list_by_tenant(
            &self,
            _tenant_id: &str,
        ) -> Result<Vec<DatasourceInfo>, BackendError> {
            Ok(vec![])
        }

        async fn find_detail_by_id(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<DatasourceDetail>, BackendError> {
            Ok(None)
        }

        async fn delete(&self, _id: &str, _tenant_id: &str) -> Result<bool, BackendError> {
            Ok(true)
        }

        async fn count_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn count_healthy_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn get_connection_info(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<dts_backend::repo::RawConnectionInfo>, BackendError> {
            Ok(None)
        }

        async fn exists(&self, id: &str, _tenant_id: &str) -> Result<bool, BackendError> {
            Ok(self.existing_ids.lock().unwrap().contains(&id.to_string()))
        }
    }

    // ============================================================
    // Helpers
    // ============================================================

    fn make_task_detail(id: &str, status: &str) -> TaskDetail {
        TaskDetail {
            id: id.to_string(),
            name: "Test Task".to_string(),
            description: None,
            source_id: "source-1".to_string(),
            target_id: "target-1".to_string(),
            source_name: "Source DB".to_string(),
            source_type: "mysql".to_string(),
            target_name: "Target DB".to_string(),
            target_type: "postgresql".to_string(),
            cron_expr: None,
            status: status.to_string(),
            config_json: "{}".to_string(),
            created_by: "admin".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            recent_runs: vec![],
        }
    }

    // ============================================================
    // create 测试
    // ============================================================

    /// 源数据源不存在时拒绝创建
    #[tokio::test]
    async fn create_rejects_when_source_does_not_exist() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());
        // 只添加 target 为存在，source 不存在
        ds_repo.add_existing("target-1");

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "sync-task",
                None,
                "source-1",
                "target-1",
                None,
                "{}",
                "admin",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("源数据源"),
            "expected error about source datasource, got: {err}"
        );
    }

    /// 目标数据源不存在时拒绝创建
    #[tokio::test]
    async fn create_rejects_when_target_does_not_exist() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());
        ds_repo.add_existing("source-1");
        // target-1 未添加

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "sync-task",
                None,
                "source-1",
                "target-1",
                None,
                "{}",
                "admin",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("目标数据源"),
            "expected error about target datasource, got: {err}"
        );
    }

    /// 源和目标数据源都存在时创建成功
    #[tokio::test]
    async fn create_succeeds_when_both_datasources_exist() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());
        ds_repo.add_existing("source-1");
        ds_repo.add_existing("target-1");

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "sync-task",
                Some("a sync task"),
                "source-1",
                "target-1",
                Some("0 */6 * * *"),
                r#"{"mode":"full"}"#,
                "admin",
            )
            .await;

        assert!(result.is_ok());
        let id = result.unwrap();
        assert_eq!(id.len(), 36);

        let params = task_repo
            .last_create_params()
            .expect("create should have been called");
        assert_eq!(params.tenant_id, "tenant-1");
        assert_eq!(params.name, "sync-task");
        assert_eq!(params.description.as_deref(), Some("a sync task"));
        assert_eq!(params.source_id, "source-1");
        assert_eq!(params.target_id, "target-1");
        assert_eq!(params.cron_expr.as_deref(), Some("0 */6 * * *"));
        assert_eq!(params.config_json, r#"{"mode":"full"}"#);
        assert_eq!(params.created_by, "admin");
    }

    // ============================================================
    // list_by_tenant 测试
    // ============================================================

    #[tokio::test]
    async fn list_by_tenant_delegates_to_repo() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_list_result(vec![]);

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.list_by_tenant("tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ============================================================
    // update_status 测试
    // ============================================================

    #[tokio::test]
    async fn update_status_accepts_draft_to_running() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "draft")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "running").await;

        assert!(result.is_ok());
        let calls = task_repo.update_status_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            (
                "task-1".to_string(),
                "tenant-1".to_string(),
                "running".to_string()
            )
        );
    }

    #[tokio::test]
    async fn update_status_accepts_running_to_paused() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "running")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "paused").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_status_accepts_running_to_completed() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "running")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "completed").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_status_accepts_running_to_failed() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "running")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "failed").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_status_rejects_completed_to_draft() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "completed")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "draft").await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("无效的状态转换"),
            "expected invalid transition error, got: {err}"
        );
    }

    #[tokio::test]
    async fn update_status_rejects_failed_to_running() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "failed")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "running").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_status_rejects_paused_to_running() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());

        task_repo.set_find_detail_result(Some(make_task_detail("task-1", "paused")));

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc.update_status("task-1", "tenant-1", "running").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_status_rejects_when_task_not_found() {
        let task_repo = Arc::new(MockTaskRepo::new());
        let ds_repo = Arc::new(MockDatasourceRepo::new());
        // find_detail_result 默认为 None

        let svc = TaskService::new(task_repo.clone(), ds_repo.clone());
        let result = svc
            .update_status("nonexistent", "tenant-1", "running")
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("不存在"),
            "expected 'task not found' error, got: {err}"
        );
    }
}
