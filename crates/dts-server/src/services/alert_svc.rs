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

use anyhow::anyhow;
use dts_backend::repo::AlertRepo;
use dts_backend::types::*;
use uuid::Uuid;

/// 允许的告警指标类型
const VALID_METRIC_TYPES: &[&str] = &["sync_delay", "error_count", "row_diff", "bytes_diff"];

/// 允许的比较运算符
const VALID_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq"];

/// 告警规则管理服务
#[derive(Clone)]
pub struct AlertService {
    alert_repo: Arc<dyn AlertRepo>,
}

impl AlertService {
    pub fn new(alert_repo: Arc<dyn AlertRepo>) -> Self {
        Self { alert_repo }
    }

    /// 创建告警规则 — 校验指标类型和运算符
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant_id: &str,
        task_id: Option<&str>,
        name: &str,
        metric_type: &str,
        operator: &str,
        threshold: f64,
        notify_channels: &str,
    ) -> anyhow::Result<String> {
        // 校验指标类型
        if !VALID_METRIC_TYPES.contains(&metric_type) {
            return Err(anyhow!(
                "无效的指标类型 '{metric_type}'，允许的值: {VALID_METRIC_TYPES:?}",
            ));
        }

        // 校验运算符
        if !VALID_OPERATORS.contains(&operator) {
            return Err(anyhow!(
                "无效的运算符 '{operator}'，允许的值: {VALID_OPERATORS:?}",
            ));
        }

        let id = Uuid::new_v4().to_string();

        let params = CreateAlertParams {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            name: name.to_string(),
            metric_type: metric_type.to_string(),
            operator: operator.to_string(),
            threshold,
            notify_channels: notify_channels.to_string(),
        };

        self.alert_repo.create(&params).await?;

        tracing::info!("告警规则 '{name}' 创建成功");

        Ok(id)
    }

    /// 获取租户下的所有告警规则
    pub async fn list_by_tenant(&self, tenant_id: &str) -> anyhow::Result<Vec<AlertInfo>> {
        Ok(self.alert_repo.list_by_tenant(tenant_id).await?)
    }

    /// 切换告警规则启用/禁用状态
    pub async fn toggle(&self, id: &str, tenant_id: &str) -> anyhow::Result<bool> {
        Ok(self.alert_repo.toggle(id, tenant_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::BackendError;

    // ============================================================
    // Mock AlertRepo
    // ============================================================

    struct MockAlertRepo {
        last_create_params: Mutex<Option<CreateAlertParams>>,
        list_result: Mutex<Vec<AlertInfo>>,
        toggle_calls: Mutex<Vec<(String, String)>>,
        toggle_result: Mutex<bool>,
    }

    impl MockAlertRepo {
        fn new() -> Self {
            Self {
                last_create_params: Mutex::new(None),
                list_result: Mutex::new(vec![]),
                toggle_calls: Mutex::new(vec![]),
                toggle_result: Mutex::new(true),
            }
        }

        fn set_list_result(&self, alerts: Vec<AlertInfo>) {
            *self.list_result.lock().expect("Mutex poisoned") = alerts;
        }

        fn last_create_params(&self) -> Option<CreateAlertParams> {
            self.last_create_params
                .lock()
                .expect("Mutex poisoned")
                .clone()
        }

        fn toggle_calls(&self) -> Vec<(String, String)> {
            self.toggle_calls.lock().expect("Mutex poisoned").clone()
        }
    }

    #[async_trait]
    impl AlertRepo for MockAlertRepo {
        async fn create(&self, params: &CreateAlertParams) -> Result<String, BackendError> {
            *self.last_create_params.lock().expect("Mutex poisoned") = Some(params.clone());
            Ok(params.id.clone())
        }

        async fn list_by_tenant(&self, _tenant_id: &str) -> Result<Vec<AlertInfo>, BackendError> {
            Ok(self.list_result.lock().expect("Mutex poisoned").clone())
        }

        async fn toggle(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
            self.toggle_calls
                .lock()
                .unwrap()
                .push((id.to_string(), tenant_id.to_string()));
            Ok(*self.toggle_result.lock().expect("Mutex poisoned"))
        }
    }

    // ============================================================
    // create 测试
    // ============================================================

    /// 无效的 metric_type 应被拒绝
    #[tokio::test]
    async fn create_rejects_invalid_metric_type() {
        let repo = Arc::new(MockAlertRepo::new());

        let svc = AlertService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                None,
                "test alert",
                "invalid_metric",
                "gt",
                10.0,
                r#"["email"]"#,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("无效的指标类型"),
            "expected metric type error, got: {err}"
        );
    }

    /// 无效的 operator 应被拒绝
    #[tokio::test]
    async fn create_rejects_invalid_operator() {
        let repo = Arc::new(MockAlertRepo::new());

        let svc = AlertService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                None,
                "test alert",
                "sync_delay",
                "invalid_op",
                10.0,
                r#"["email"]"#,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("无效的运算符"),
            "expected operator error, got: {err}"
        );
    }

    /// 所有有效的 metric_type 值都应被接受
    #[tokio::test]
    async fn create_accepts_valid_metric_types() {
        for &metric_type in VALID_METRIC_TYPES {
            let repo = Arc::new(MockAlertRepo::new());

            let svc = AlertService::new(repo.clone());
            let result = svc
                .create(
                    "tenant-1",
                    Some("task-1"),
                    "test alert",
                    metric_type,
                    "gt",
                    10.0,
                    r#"["email","sms"]"#,
                )
                .await;

            assert!(
                result.is_ok(),
                "expected metric_type '{metric_type}' to be accepted"
            );
        }
    }

    /// 所有有效的 operator 值都应被接受
    #[tokio::test]
    async fn create_accepts_valid_operators() {
        for &operator in VALID_OPERATORS {
            let repo = Arc::new(MockAlertRepo::new());

            let svc = AlertService::new(repo.clone());
            let result = svc
                .create(
                    "tenant-1",
                    None,
                    "test alert",
                    "error_count",
                    operator,
                    5.0,
                    r#"["email"]"#,
                )
                .await;

            assert!(
                result.is_ok(),
                "expected operator '{operator}' to be accepted"
            );
        }
    }

    /// 创建成功时返回 UUID 格式的 ID
    #[tokio::test]
    async fn create_returns_valid_uuid() {
        let repo = Arc::new(MockAlertRepo::new());

        let svc = AlertService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                Some("task-1"),
                "sync delay alert",
                "sync_delay",
                "gt",
                300.0,
                r#"["email","webhook"]"#,
            )
            .await;

        assert!(result.is_ok());
        let id = result.unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert_eq!(params.tenant_id, "tenant-1");
        assert_eq!(params.name, "sync delay alert");
        assert_eq!(params.metric_type, "sync_delay");
        assert_eq!(params.operator, "gt");
        assert!((params.threshold - 300.0).abs() < f64::EPSILON);
        assert_eq!(params.notify_channels, r#"["email","webhook"]"#);
        assert_eq!(params.task_id.as_deref(), Some("task-1"));
    }

    /// task_id 为 None 时正确传递
    #[tokio::test]
    async fn create_accepts_none_task_id() {
        let repo = Arc::new(MockAlertRepo::new());

        let svc = AlertService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                None,
                "global alert",
                "error_count",
                "eq",
                0.0,
                r#"["email"]"#,
            )
            .await;

        assert!(result.is_ok());
        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert!(params.task_id.is_none());
    }

    // ============================================================
    // list_by_tenant 测试
    // ============================================================

    #[tokio::test]
    async fn list_by_tenant_delegates_to_repo() {
        let repo = Arc::new(MockAlertRepo::new());
        repo.set_list_result(vec![]);

        let svc = AlertService::new(repo.clone());
        let result = svc.list_by_tenant("tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ============================================================
    // toggle 测试
    // ============================================================

    #[tokio::test]
    async fn toggle_delegates_to_repo() {
        let repo = Arc::new(MockAlertRepo::new());

        let svc = AlertService::new(repo.clone());
        let result = svc.toggle("alert-1", "tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap());

        let calls = repo.toggle_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("alert-1".to_string(), "tenant-1".to_string()));
    }
}
