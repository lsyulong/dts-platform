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
    use dts_backend::BackendError;
    use dts_backend::repo::TaskRepo;

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
            *self.overview_result.lock().expect("Mutex poisoned") = overview;
        }
    }

    #[async_trait]
    impl MonitoringRepo for MockMonitoringRepo {
        async fn get_overview(&self, _tenant_id: &str) -> Result<MonitoringOverview, BackendError> {
            Ok(self.overview_result.lock().expect("Mutex poisoned").clone())
        }
    }

    // ============================================================
    // Mock TaskRepo (minimal for MonitoringService tests)
    // ============================================================

    struct MockTaskRepo;

    #[async_trait]
    impl TaskRepo for MockTaskRepo {
        async fn create(&self, _params: &CreateTaskParams) -> Result<String, BackendError> {
            Ok("ignored".to_string())
        }

        async fn list_by_tenant(&self, _tenant_id: &str) -> Result<Vec<TaskInfo>, BackendError> {
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

    /// 验证空状态概览（新租户无任务无数据源）
    #[tokio::test]
    async fn get_overview_returns_empty_state_for_new_tenant() {
        let monitoring_repo = Arc::new(MockMonitoringRepo::new());
        let task_repo = Arc::new(MockTaskRepo);

        // MockMonitoringRepo 默认返回全零概览

        let svc = MonitoringService::new(monitoring_repo.clone(), task_repo);
        let result = svc.get_overview("new-tenant").await;

        assert!(result.is_ok());
        let overview = result.unwrap();
        assert_eq!(overview.total_tasks, 0);
        assert_eq!(overview.running_tasks, 0);
        assert_eq!(overview.failed_tasks, 0);
        assert_eq!(overview.total_datasources, 0);
        assert_eq!(overview.healthy_datasources, 0);
        assert_eq!(overview.today_synced_rows, 0);
        assert_eq!(overview.today_synced_bytes, 0);
        assert_eq!(overview.success_rate, 0.0);
        assert!(overview.recent_runs.is_empty());
    }

    /// 验证 100% 成功率
    #[tokio::test]
    async fn get_overview_handles_perfect_success_rate() {
        let monitoring_repo = Arc::new(MockMonitoringRepo::new());
        let task_repo = Arc::new(MockTaskRepo);

        let overview = MonitoringOverview {
            total_tasks: 50,
            running_tasks: 0,
            failed_tasks: 0,
            total_datasources: 10,
            healthy_datasources: 10,
            today_synced_rows: 5_000_000,
            today_synced_bytes: 250_000_000,
            success_rate: 100.0,
            recent_runs: vec![],
        };
        monitoring_repo.set_overview(overview.clone());

        let svc = MonitoringService::new(monitoring_repo.clone(), task_repo);
        let result = svc.get_overview("tenant-perfect").await;

        assert!(result.is_ok());
        let overview = result.unwrap();
        assert!((overview.success_rate - 100.0).abs() < f64::EPSILON);
        assert_eq!(overview.failed_tasks, 0);
        assert_eq!(overview.total_tasks, 50);
    }

    /// 验证全部失败场景
    #[tokio::test]
    async fn get_overview_handles_all_failed() {
        let monitoring_repo = Arc::new(MockMonitoringRepo::new());
        let task_repo = Arc::new(MockTaskRepo);

        let overview = MonitoringOverview {
            total_tasks: 10,
            running_tasks: 0,
            failed_tasks: 10,
            total_datasources: 5,
            healthy_datasources: 5,
            today_synced_rows: 0,
            today_synced_bytes: 0,
            success_rate: 0.0,
            recent_runs: vec![],
        };
        monitoring_repo.set_overview(overview.clone());

        let svc = MonitoringService::new(monitoring_repo.clone(), task_repo);
        let result = svc.get_overview("tenant-failing").await;

        assert!(result.is_ok());
        let overview = result.unwrap();
        assert_eq!(overview.failed_tasks, 10);
        assert!((overview.success_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(overview.today_synced_rows, 0);
    }

    /// 验证大数据量场景
    #[tokio::test]
    async fn get_overview_handles_large_numbers() {
        let monitoring_repo = Arc::new(MockMonitoringRepo::new());
        let task_repo = Arc::new(MockTaskRepo);

        let overview = MonitoringOverview {
            total_tasks: i64::MAX,
            running_tasks: 0,
            failed_tasks: 0,
            total_datasources: 1000,
            healthy_datasources: 1000,
            today_synced_rows: 9_999_999_999,
            today_synced_bytes: 999_999_999_999,
            success_rate: 100.0,
            recent_runs: vec![],
        };
        monitoring_repo.set_overview(overview.clone());

        let svc = MonitoringService::new(monitoring_repo.clone(), task_repo);
        let result = svc.get_overview("tenant-large").await;

        assert!(result.is_ok());
        let overview = result.unwrap();
        assert_eq!(overview.total_tasks, i64::MAX);
        assert_eq!(overview.today_synced_rows, 9_999_999_999);
        assert_eq!(overview.today_synced_bytes, 999_999_999_999);
    }
}
