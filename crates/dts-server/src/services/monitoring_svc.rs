use std::sync::Arc;

use dts_backend::repo::MonitoringRepo;
use dts_backend::types::*;

/// 监控数据面服务
#[derive(Clone)]
pub struct MonitoringService {
    monitoring_repo: Arc<dyn MonitoringRepo>,
    #[allow(dead_code)]
    task_repo: Arc<dyn dts_backend::repo::TaskRepo>,
}

impl MonitoringService {
    pub fn new(
        monitoring_repo: Arc<dyn MonitoringRepo>,
        task_repo: Arc<dyn dts_backend::repo::TaskRepo>,
    ) -> Self {
        Self {
            monitoring_repo,
            task_repo,
        }
    }

    /// 获取租户监控概览
    pub async fn get_overview(&self, tenant_id: &str) -> anyhow::Result<MonitoringOverview> {
        Ok(self.monitoring_repo.get_overview(tenant_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::repo::TaskRepo;
    use dts_backend::BackendError;

    // ============================================================
    // Mock MonitoringRepo
    // ============================================================

    struct MockMonitoringRepo {
        overview_result: Mutex<MonitoringOverview>,
    }

    impl MockMonitoringRepo {
        fn new() -> Self {
            Self {
                overview_result: Mutex::new(MonitoringOverview {
                    total_tasks: 0,
                    running_tasks: 0,
                    failed_tasks: 0,
                    total_datasources: 0,
                    healthy_datasources: 0,
                    today_synced_rows: 0,
                    today_synced_bytes: 0,
                    success_rate: 0.0,
                    recent_runs: vec![],
                }),
            }
        }

        fn set_overview(&self, overview: MonitoringOverview) {
            *self.overview_result.lock().unwrap() = overview;
        }
    }

    #[async_trait]
    impl MonitoringRepo for MockMonitoringRepo {
        async fn get_overview(
            &self,
            _tenant_id: &str,
        ) -> Result<MonitoringOverview, BackendError> {
            Ok(self.overview_result.lock().unwrap().clone())
        }
    }

    // ============================================================
    // Mock TaskRepo (minimal for MonitoringService tests)
    // ============================================================

    struct MockTaskRepo;

    #[async_trait]
    impl TaskRepo for MockTaskRepo {
        async fn create(
            &self,
            _params: &CreateTaskParams,
        ) -> Result<String, BackendError> {
            Ok("ignored".to_string())
        }

        async fn list_by_tenant(
            &self,
            _tenant_id: &str,
        ) -> Result<Vec<TaskInfo>, BackendError> {
            Ok(vec![])
        }

        async fn find_detail_by_id(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<TaskDetail>, BackendError> {
            Ok(None)
        }

        async fn update_status(
            &self,
            _id: &str,
            _tenant_id: &str,
            _status: &str,
        ) -> Result<bool, BackendError> {
            Ok(false)
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
    // get_overview 测试
    // ============================================================

    #[tokio::test]
    async fn get_overview_delegates_to_repo() {
        let monitoring_repo = Arc::new(MockMonitoringRepo::new());
        let task_repo = Arc::new(MockTaskRepo);

        let expected = MonitoringOverview {
            total_tasks: 10,
            running_tasks: 3,
            failed_tasks: 1,
            total_datasources: 5,
            healthy_datasources: 4,
            today_synced_rows: 100_000,
            today_synced_bytes: 5_000_000,
            success_rate: 0.95,
            recent_runs: vec![],
        };
        monitoring_repo.set_overview(expected.clone());

        let svc = MonitoringService::new(monitoring_repo.clone(), task_repo);
        let result = svc.get_overview("tenant-1").await;

        assert!(result.is_ok());
        let overview = result.unwrap();
        assert_eq!(overview.total_tasks, 10);
        assert_eq!(overview.running_tasks, 3);
        assert_eq!(overview.failed_tasks, 1);
        assert_eq!(overview.total_datasources, 5);
        assert_eq!(overview.healthy_datasources, 4);
        assert_eq!(overview.today_synced_rows, 100_000);
        assert_eq!(overview.today_synced_bytes, 5_000_000);
        assert!((overview.success_rate - 0.95).abs() < f64::EPSILON);
    }
}
