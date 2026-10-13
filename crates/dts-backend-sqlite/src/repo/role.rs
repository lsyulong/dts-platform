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
use chrono::NaiveDateTime;
use dts_backend::{BackendError, repo::RoleRepo, repo::role::DefaultRoles, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::Row;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct SqliteRoleRepo {
    pool: SqlitePool,
}

impl SqliteRoleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoleRepo for SqliteRoleRepo {
    async fn create_with_permissions(
        &self,
        params: &CreateRoleParams,
    ) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO role (id, tenant_id, name, description, is_system) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(params.is_system)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        for perm in &params.permissions {
            sqlx::query("INSERT INTO role_permission (role_id, permission) VALUES (?1, ?2)")
                .bind(&params.id)
                .bind(perm)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }

        Ok(params.id.clone())
    }

    async fn find_user_role_names(&self, user_id: &str) -> Result<Vec<String>, BackendError> {
        sqlx::query_scalar(
            "SELECT r.name FROM role r
             INNER JOIN user_role ur ON r.id = ur.role_id
             WHERE ur.user_id = ?1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn find_user_permissions(&self, user_id: &str) -> Result<Vec<String>, BackendError> {
        sqlx::query_scalar(
            "SELECT DISTINCT rp.permission FROM role_permission rp
             INNER JOIN user_role ur ON rp.role_id = ur.role_id
             WHERE ur.user_id = ?1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn create_default_roles(&self, tenant_id: &str) -> Result<DefaultRoles, BackendError> {
        let admin_id = self
            .create_with_permissions(&CreateRoleParams {
                id: Uuid::new_v4().to_string(),
                tenant_id: tenant_id.into(),
                name: "tenant_admin".into(),
                description: "租户管理员".into(),
                is_system: true,
                permissions: dts_backend::PERMISSIONS
                    .iter()
                    .filter(|permission| **permission != "tenant:create")
                    .map(|permission| (*permission).to_owned())
                    .collect(),
            })
            .await?;

        let dev_id = self
            .create_with_permissions(&CreateRoleParams {
                id: Uuid::new_v4().to_string(),
                tenant_id: tenant_id.into(),
                name: "developer".into(),
                description: "开发运维".into(),
                is_system: true,
                permissions: vec![
                    "task:read".into(),
                    "task:write".into(),
                    "task:execute".into(),
                    "datasource:read".into(),
                    "datasource:write".into(),
                    "monitor:read".into(),
                    "alert:read".into(),
                ],
            })
            .await?;

        let viewer_id = self
            .create_with_permissions(&CreateRoleParams {
                id: Uuid::new_v4().to_string(),
                tenant_id: tenant_id.into(),
                name: "viewer".into(),
                description: "只读用户".into(),
                is_system: true,
                permissions: vec![
                    "task:read".into(),
                    "datasource:read".into(),
                    "monitor:read".into(),
                    "alert:read".into(),
                ],
            })
            .await?;

        Ok(DefaultRoles {
            admin_role_id: admin_id,
            developer_role_id: dev_id,
            viewer_role_id: viewer_id,
        })
    }

    async fn list_all(&self, tenant_id: Option<&str>) -> Result<Vec<RoleListInfo>, BackendError> {
        let mut sql = String::from(
            "SELECT r.id, r.name, r.tenant_id, r.description, r.is_system, r.created_at \
             FROM role r WHERE 1=1",
        );
        if let Some(tid) = tenant_id {
            sql.push_str(&format!(" AND r.tenant_id = '{}'", tid.replace('\'', "''")));
        }
        sql.push_str(" ORDER BY r.created_at DESC");

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let mut roles = Vec::new();
        for r in &rows {
            let rid: String = r.get(0);
            let permissions: Vec<String> =
                sqlx::query_scalar("SELECT permission FROM role_permission WHERE role_id = ?1")
                    .bind(&rid)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(map_sqlx_error)?;

            let created_at_str: String = r.get(5);
            let created_at = NaiveDateTime::parse_from_str(&created_at_str, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc().to_rfc3339())
                .unwrap_or_else(|_| created_at_str);

            roles.push(RoleListInfo {
                id: rid,
                name: r.get(1),
                tenant_id: r.get(2),
                description: r.get(3),
                is_system: r.get::<i32, _>(4) != 0,
                permissions,
                created_at,
            });
        }
        Ok(roles)
    }

    async fn update(&self, role_id: &str, params: &UpdateRoleParams) -> Result<(), BackendError> {
        if let Some(ref name) = params.name {
            sqlx::query("UPDATE role SET name = ?1 WHERE id = ?2")
                .bind(name)
                .bind(role_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        if let Some(ref permissions) = params.permissions {
            sqlx::query("DELETE FROM role_permission WHERE role_id = ?1")
                .bind(role_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
            for perm in permissions {
                sqlx::query("INSERT INTO role_permission (role_id, permission) VALUES (?1, ?2)")
                    .bind(role_id)
                    .bind(perm)
                    .execute(&self.pool)
                    .await
                    .map_err(map_sqlx_error)?;
            }
        }
        Ok(())
    }

    async fn delete(&self, role_id: &str) -> Result<(), BackendError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_sqlx_error)?;
        sqlx::query("DELETE FROM role_permission WHERE role_id = ?1")
            .bind(role_id)
            .execute(&mut *transaction)
            .await
            .map_err(super::product_resource::map_reference_error)?;
        sqlx::query("DELETE FROM user_role WHERE role_id = ?1")
            .bind(role_id)
            .execute(&mut *transaction)
            .await
            .map_err(super::product_resource::map_reference_error)?;
        sqlx::query("DELETE FROM role WHERE id = ?1")
            .bind(role_id)
            .execute(&mut *transaction)
            .await
            .map_err(super::product_resource::map_reference_error)?;
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn find_all_permissions(&self) -> Result<Vec<String>, BackendError> {
        Ok(dts_backend::PERMISSIONS
            .iter()
            .map(|permission| (*permission).to_owned())
            .collect())
    }
}
