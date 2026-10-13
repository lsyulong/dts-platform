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

use async_trait::async_trait;
use dts_backend::{BackendError, repo::SeedRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::repo::{SqliteDatasourceRepo, SqliteRoleRepo, SqliteTenantRepo, SqliteUserRepo};
use dts_backend::repo::{DatasourceRepo, RoleRepo, TenantRepo, UserRepo};
use dts_backend::types::*;

pub struct SqliteSeedRepo {
    pool: SqlitePool,
}

impl SqliteSeedRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn seed_internal(
        &self,
        admin_password: &str,
        include_demo: bool,
    ) -> Result<(), BackendError> {
        let admin_password_hash = hash_password_sync(admin_password)?;
        tracing::info!("初始化租户和管理员...");

        let tenant_repo = SqliteTenantRepo::new(self.pool.clone());
        let role_repo = SqliteRoleRepo::new(self.pool.clone());
        let user_repo = SqliteUserRepo::new(self.pool.clone());
        let ds_repo = SqliteDatasourceRepo::new(self.pool.clone());

        // 1. 创建默认租户
        let tenant_id = Uuid::new_v4().to_string();
        let tenant_name = "默认租户";
        let tenant_slug = "default";

        tenant_repo
            .create(&CreateTenantParams {
                id: tenant_id.clone(),
                name: tenant_name.into(),
                slug: tenant_slug.into(),
            })
            .await?;

        // 2. 创建默认角色
        let roles = role_repo.create_default_roles(&tenant_id).await?;

        // 3. 创建管理员用户
        let user_id = Uuid::new_v4().to_string();

        user_repo
            .create(&CreateUserParams {
                id: user_id.clone(),
                tenant_id: tenant_id.clone(),
                username: "admin".into(),
                email: "admin@dts.local".into(),
                password_hash: admin_password_hash,
                display_name: Some("系统管理员".into()),
            })
            .await?;

        let admin_role_id = if include_demo {
            roles.admin_role_id
        } else {
            role_repo
                .create_with_permissions(&CreateRoleParams {
                    id: Uuid::new_v4().to_string(),
                    tenant_id: tenant_id.clone(),
                    name: "platform_admin".into(),
                    description: "平台管理员".into(),
                    is_system: true,
                    permissions: dts_backend::PERMISSIONS
                        .iter()
                        .map(|permission| (*permission).to_owned())
                        .collect(),
                })
                .await?
        };

        user_repo.assign_role(&user_id, &admin_role_id).await?;

        if include_demo {
            // 4. 创建演示数据源
            let mysql_id = Uuid::new_v4().to_string();
            ds_repo
                .create(&CreateDatasourceParams {
                    id: mysql_id,
                    tenant_id: tenant_id.clone(),
                    name: "生产 MySQL".into(),
                    db_type: "MySQL".into(),
                    host: "192.168.1.100".into(),
                    port: 3306,
                    username: Some("root".into()),
                    encrypted_password: Some("enc:demo123".into()),
                    database_name: Some("ecommerce".into()),
                    created_by: user_id.clone(),
                })
                .await?;

            let pg_id = Uuid::new_v4().to_string();
            ds_repo
                .create(&CreateDatasourceParams {
                    id: pg_id,
                    tenant_id: tenant_id.clone(),
                    name: "分析 PostgreSQL".into(),
                    db_type: "PostgreSQL".into(),
                    host: "192.168.1.101".into(),
                    port: 5432,
                    username: Some("analyst".into()),
                    encrypted_password: Some("enc:demo456".into()),
                    database_name: Some("warehouse".into()),
                    created_by: user_id,
                })
                .await?;
        }

        tracing::info!("管理员初始化完成: 租户=default, 管理员=admin");
        Ok(())
    }
}

#[async_trait]
impl SeedRepo for SqliteSeedRepo {
    async fn has_data(&self) -> Result<bool, BackendError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(count > 0)
    }

    async fn seed(&self) -> Result<(), BackendError> {
        self.seed_internal("admin123", true).await
    }

    async fn seed_bootstrap(&self, admin_password: &str) -> Result<(), BackendError> {
        if admin_password.len() < 12 {
            return Err(BackendError::Configuration(
                "首次管理员密码必须至少 12 字节".into(),
            ));
        }
        self.seed_internal(admin_password, false).await
    }
}

/// 使用 Argon2 哈希密码（同步方法，用于种子数据初始化）
fn hash_password_sync(password: &str) -> Result<String, BackendError> {
    use argon2::{PasswordHasher, password_hash::SaltString};
    use rand_core::OsRng;
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| BackendError::Internal(format!("密码哈希失败: {e}")))?;
    Ok(hash.to_string())
}
