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

use dts_backend::BackendError;
use dts_backend::repo::{DatasourceRepo, TaskRepo};
use dts_backend::types::*;
use uuid::Uuid;

#[path = "task_svc/config.rs"]
mod config;
pub use config::{PipelineConfig, default_batch_size};

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
    ) -> Result<String, BackendError> {
        if source_id == target_id {
            return Err(BackendError::ConstraintViolation(
                "源端和目标端不能使用同一个数据源".into(),
            ));
        }
        let source_type = self.datasource_type(source_id, tenant_id, "源").await?;
        let target_type = self.datasource_type(target_id, tenant_id, "目标").await?;
        validate_engine_pair(&source_type, &target_type)?;

        if name.trim().is_empty() {
            return Err(BackendError::ConstraintViolation("任务名称不能为空".into()));
        }
        if cron_expr.is_some_and(|cron| !cron.trim().is_empty()) {
            return Err(BackendError::ConstraintViolation(
                "当前 Pipeline 仅支持手动运行；定时调度尚未绑定".into(),
            ));
        }
        let config = PipelineConfig::parse(config_json)?;
        let config_json = serde_json::to_string(&config)
            .map_err(|e| BackendError::Internal(format!("序列化任务配置失败: {e}")))?;

        let id = Uuid::new_v4().to_string();

        let params = CreateTaskParams {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            cron_expr: cron_expr.map(|s| s.to_string()),
            config_json,
            created_by: created_by.to_string(),
        };

        self.task_repo.create(&params).await?;

        tracing::info!("同步任务 '{name}' 创建成功");

        Ok(id)
    }

    /// 获取租户下的所有同步任务
    pub async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<TaskInfo>, BackendError> {
        self.task_repo.list_by_tenant(tenant_id).await
    }

    async fn datasource_type(
        &self,
        id: &str,
        tenant_id: &str,
        role: &str,
    ) -> Result<String, BackendError> {
        let datasource = self
            .datasource_repo
            .find_detail_by_id(id, tenant_id)
            .await?
            .ok_or_else(|| {
                BackendError::NotFound(format!("{role}数据源 '{id}' 不存在或不属于该租户"))
            })?;
        Ok(datasource.db_type.trim().to_ascii_lowercase())
    }
}

fn validate_engine_pair(source: &str, target: &str) -> Result<(), BackendError> {
    let relational = |kind: &str| matches!(kind, "mysql" | "postgresql" | "postgres" | "pg");
    let analytical = |kind: &str| matches!(kind, "duckdb" | "swanlake");
    if (relational(source) && relational(target))
        || ((relational(source) || analytical(source)) && analytical(target))
    {
        Ok(())
    } else {
        Err(BackendError::ConstraintViolation(format!(
            "当前没有可执行 {source} 到 {target} 的同步引擎"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct MockTaskRepo {
        created: Mutex<Vec<CreateTaskParams>>,
    }
    #[async_trait]
    impl TaskRepo for MockTaskRepo {
        async fn create(&self, params: &CreateTaskParams) -> Result<String, BackendError> {
            self.created.lock().unwrap().push(params.clone());
            Ok(params.id.clone())
        }
        async fn list_by_tenant(&self, _: &str) -> Result<Vec<TaskInfo>, BackendError> {
            Ok(vec![])
        }
        async fn find_detail_by_id(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<TaskDetail>, BackendError> {
            Ok(None)
        }
        async fn update_status(&self, _: &str, _: &str, _: &str) -> Result<bool, BackendError> {
            Ok(true)
        }
        async fn count_by_tenant(&self, _: &str) -> Result<i64, BackendError> {
            Ok(0)
        }
        async fn count_by_status(&self, _: &str, _: &str) -> Result<i64, BackendError> {
            Ok(0)
        }
        async fn find_recent_runs(
            &self,
            _: &str,
            _: i64,
        ) -> Result<Vec<TaskRunInfo>, BackendError> {
            Ok(vec![])
        }
    }

    #[derive(Default)]
    struct MockDatasourceRepo {
        types: HashMap<String, String>,
    }
    #[async_trait]
    impl DatasourceRepo for MockDatasourceRepo {
        async fn create(&self, _: &CreateDatasourceParams) -> Result<String, BackendError> {
            Ok("unused".into())
        }
        async fn list_by_tenant(&self, _: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
            Ok(vec![])
        }
        async fn find_detail_by_id(
            &self,
            id: &str,
            _: &str,
        ) -> Result<Option<DatasourceDetail>, BackendError> {
            Ok(self.types.get(id).map(|kind| DatasourceDetail {
                id: id.into(),
                name: id.into(),
                db_type: kind.clone(),
                host: "localhost".into(),
                port: 5432,
                username: Some("user".into()),
                password_masked: None,
                database_name: Some("db".into()),
                extra_params: None,
                health_status: "unknown".into(),
                last_test_at: None,
                created_at: "".into(),
                updated_at: "".into(),
            }))
        }
        async fn get_connection_info(
            &self,
            _: &str,
            _: &str,
        ) -> Result<Option<dts_backend::repo::RawConnectionInfo>, BackendError> {
            Ok(None)
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
        async fn exists(&self, id: &str, _: &str) -> Result<bool, BackendError> {
            Ok(self.types.contains_key(id))
        }
    }

    fn harness(
        source_type: Option<&str>,
        target_type: Option<&str>,
    ) -> (TaskService, Arc<MockTaskRepo>) {
        let tasks = Arc::new(MockTaskRepo::default());
        let mut types = HashMap::new();
        if let Some(kind) = source_type {
            types.insert("source".into(), kind.into());
        }
        if let Some(kind) = target_type {
            types.insert("target".into(), kind.into());
        }
        (
            TaskService::new(tasks.clone(), Arc::new(MockDatasourceRepo { types })),
            tasks,
        )
    }
    const VALID_CONFIG: &str = r#"{"table_mappings":[{"source_table":"orders","target_table":"fact_orders","target_schema":"mart"}],"sync_mode":"full","batch_size":500}"#;

    #[tokio::test]
    async fn create_rejects_missing_or_unsupported_datasource_without_persisting() {
        for (source, target) in [
            (None, Some("pg")),
            (Some("mysql"), None),
            (Some("sqlite"), Some("pg")),
            (Some("duckdb"), Some("pg")),
        ] {
            let (service, tasks) = harness(source, target);
            let result = service
                .create(
                    "tenant",
                    "Pipeline",
                    None,
                    "source",
                    "target",
                    None,
                    VALID_CONFIG,
                    "admin",
                )
                .await;
            assert!(result.is_err());
            assert!(tasks.created.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn create_accepts_relational_source_into_duckdb() {
        let (service, tasks) = harness(Some("mysql"), Some("duckdb"));
        let result = service
            .create(
                "tenant",
                "Warehouse load",
                None,
                "source",
                "target",
                None,
                VALID_CONFIG,
                "admin",
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(tasks.created.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_persists_validated_config_without_losing_schema_or_batch_size() {
        let (service, tasks) = harness(Some("mysql"), Some("postgresql"));
        let id = service
            .create(
                "tenant",
                "Pipeline",
                Some("description"),
                "source",
                "target",
                None,
                VALID_CONFIG,
                "admin",
            )
            .await
            .unwrap();
        let created = tasks.created.lock().unwrap();
        assert_eq!(created[0].id, id);
        assert_eq!(created[0].tenant_id, "tenant");
        let config = PipelineConfig::parse(&created[0].config_json).unwrap();
        assert_eq!(config.batch_size, 500);
        assert_eq!(
            config.table_mappings[0].target_schema.as_deref(),
            Some("mart")
        );
    }

    #[tokio::test]
    async fn invalid_config_and_empty_name_never_create_pipeline() {
        let (service, tasks) = harness(Some("mysql"), Some("pg"));
        for (name, config) in [
            ("Pipeline", "{}"),
            ("Pipeline", "not json"),
            ("  ", VALID_CONFIG),
        ] {
            assert!(
                service
                    .create(
                        "tenant", name, None, "source", "target", None, config, "admin"
                    )
                    .await
                    .is_err()
            );
        }
        assert!(tasks.created.lock().unwrap().is_empty());
    }
}
