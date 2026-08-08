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

use dts_backend::repo::DatasourceRepo;
use dts_backend::types::*;
use uuid::Uuid;

/// 数据源管理服务
#[derive(Clone)]
pub struct DatasourceService {
    datasource_repo: Arc<dyn DatasourceRepo>,
}

impl DatasourceService {
    pub fn new(datasource_repo: Arc<dyn DatasourceRepo>) -> Self {
        Self { datasource_repo }
    }

    /// 创建数据源 — 密码使用 "enc:" 前缀进行占位加密（后续替换为 AES-GCM）
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        db_type: &str,
        host: &str,
        port: i64,
        username: Option<&str>,
        password: Option<&str>,
        database_name: Option<&str>,
        created_by: &str,
    ) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        let encrypted_password = password.map(|p| format!("enc:{p}"));

        let params = CreateDatasourceParams {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            db_type: db_type.to_string(),
            host: host.to_string(),
            port,
            username: username.map(|s| s.to_string()),
            encrypted_password,
            database_name: database_name.map(|s| s.to_string()),
            created_by: created_by.to_string(),
        };

        self.datasource_repo.create(&params).await?;

        tracing::info!("数据源 '{name}' 创建成功");

        Ok(id)
    }

    /// 获取租户下的所有数据源
    pub async fn list_by_tenant(&self, tenant_id: &str) -> anyhow::Result<Vec<DatasourceInfo>> {
        Ok(self.datasource_repo.list_by_tenant(tenant_id).await?)
    }

    /// 检查数据源是否存在且属于该租户
    pub async fn exists(&self, id: &str, tenant_id: &str) -> anyhow::Result<bool> {
        Ok(self.datasource_repo.exists(id, tenant_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use dts_backend::BackendError;

    /// 手动 mock DatasourceRepo — 捕获 create 调用参数，返回预设结果
    struct MockDatasourceRepo {
        last_create_params: Mutex<Option<CreateDatasourceParams>>,
        exists_result: Mutex<bool>,
    }

    impl MockDatasourceRepo {
        fn new() -> Self {
            Self {
                last_create_params: Mutex::new(None),
                exists_result: Mutex::new(false),
            }
        }

        fn set_exists_result(&self, value: bool) {
            *self.exists_result.lock().expect("Mutex poisoned") = value;
        }

        fn last_create_params(&self) -> Option<CreateDatasourceParams> {
            self.last_create_params.lock().expect("Mutex poisoned").clone()
        }
    }

    #[async_trait]
    impl DatasourceRepo for MockDatasourceRepo {
        async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError> {
            *self.last_create_params.lock().expect("Mutex poisoned") = Some(params.clone());
            Ok(params.id.clone())
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

        async fn get_connection_info(
            &self,
            _id: &str,
            _tenant_id: &str,
        ) -> Result<Option<dts_backend::repo::RawConnectionInfo>, BackendError> {
            Ok(None)
        }

        async fn count_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn count_healthy_by_tenant(&self, _tenant_id: &str) -> Result<i64, BackendError> {
            Ok(0)
        }

        async fn exists(&self, _id: &str, _tenant_id: &str) -> Result<bool, BackendError> {
            Ok(*self.exists_result.lock().expect("Mutex poisoned"))
        }
    }

    /// 验证密码使用 "enc:" 前缀进行占位加密
    #[tokio::test]
    async fn create_encrypts_password_with_enc_prefix() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "mysql-db",
                "mysql",
                "localhost",
                3306,
                Some("root"),
                Some("secret123"),
                Some("mydb"),
                "admin-user",
            )
            .await;

        assert!(result.is_ok());

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert_eq!(
            params.encrypted_password.as_deref(),
            Some("enc:secret123"),
            "password should be prefixed with 'enc:'"
        );
    }

    /// 验证无密码时 encrypted_password 为 None
    #[tokio::test]
    async fn create_sets_encrypted_password_to_none_when_password_absent() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "pg-db",
                "postgresql",
                "db.example.com",
                5432,
                None,
                None,
                None,
                "admin-user",
            )
            .await;

        assert!(result.is_ok());

        let params = repo
            .last_create_params()
            .expect("create should have been called");
        assert!(
            params.encrypted_password.is_none(),
            "encrypted_password should be None when no password provided"
        );
    }

    /// 验证 create 返回 UUID v4 格式的 ID
    #[tokio::test]
    async fn create_returns_non_empty_id() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc
            .create(
                "tenant-1",
                "pg-db",
                "postgresql",
                "db.example.com",
                5432,
                None,
                None,
                None,
                "admin-user",
            )
            .await;

        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        // UUID v4 格式: 36 字符 + 4 个连字符
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    }

    /// 验证 list_by_tenant 正确委托给 repo
    #[tokio::test]
    async fn list_by_tenant_delegates_to_repo() {
        let repo = Arc::new(MockDatasourceRepo::new());

        let svc = DatasourceService::new(repo.clone());
        let result = svc.list_by_tenant("tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// 验证 exists 正确委托给 repo（存在时返回 true）
    #[tokio::test]
    async fn exists_delegates_to_repo() {
        let repo = Arc::new(MockDatasourceRepo::new());
        repo.set_exists_result(true);

        let svc = DatasourceService::new(repo.clone());
        let result = svc.exists("ds-1", "tenant-1").await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    /// 验证 exists 返回 false（数据源不存在）
    #[tokio::test]
    async fn exists_returns_false_when_not_found() {
        let repo = Arc::new(MockDatasourceRepo::new());
        // exists_result 默认为 false

        let svc = DatasourceService::new(repo.clone());
        let result = svc.exists("ds-missing", "tenant-1").await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
