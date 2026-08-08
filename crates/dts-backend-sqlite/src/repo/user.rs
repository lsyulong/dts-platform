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
use dts_backend::{BackendError, repo::UserRepo, repo::user::UserDetail, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::Row;
use sqlx::SqlitePool;

pub struct SqliteUserRepo {
    pool: SqlitePool,
}

impl SqliteUserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 批量查询用户角色映射（避免 N+1）
    async fn batch_get_roles(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<String>>, BackendError> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let placeholders: Vec<String> = user_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT ur.user_id, r.name FROM role r \
             INNER JOIN user_role ur ON r.id = ur.role_id \
             WHERE ur.user_id IN ({})",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&sql);
        for uid in user_ids {
            query = query.bind(uid);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;

        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for row in &rows {
            let user_id: String = row.get(0);
            let role_name: String = row.get(1);
            map.entry(user_id).or_default().push(role_name);
        }
        Ok(map)
    }
}

#[async_trait]
impl UserRepo for SqliteUserRepo {
    async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO \"user\" (id, tenant_id, username, email, password_hash, display_name, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active')",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.username)
        .bind(&params.email)
        .bind(&params.password_hash)
        .bind(&params.display_name)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn find_by_tenant_and_username(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, BackendError> {
        let row = sqlx::query(
            "SELECT id, tenant_id, username, email, password_hash, display_name, status
             FROM \"user\" WHERE tenant_id = ?1 AND username = ?2",
        )
        .bind(tenant_id)
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(Some(UserRecord {
                id: r.get(0),
                tenant_id: r.get(1),
                username: r.get(2),
                email: r.get(3),
                password_hash: r.get(4),
                display_name: r.get(5),
                status: r.get(6),
            })),
            None => Ok(None),
        }
    }

    async fn find_detail_by_id(&self, user_id: &str) -> Result<Option<UserDetail>, BackendError> {
        let row = sqlx::query(
            "SELECT u.id, u.username, u.email, u.display_name, u.tenant_id,
                    t.name as tenant_name
             FROM \"user\" u
             JOIN tenant t ON u.tenant_id = t.id
             WHERE u.id = ?1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => {
                let uid: String = r.get(0);
                let roles: Vec<String> = sqlx::query_scalar(
                    "SELECT r.name FROM role r
                     INNER JOIN user_role ur ON r.id = ur.role_id
                     WHERE ur.user_id = ?1",
                )
                .bind(&uid)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

                let permissions: Vec<String> = sqlx::query_scalar(
                    "SELECT DISTINCT rp.permission FROM role_permission rp
                     INNER JOIN user_role ur ON rp.role_id = ur.role_id
                     WHERE ur.user_id = ?1",
                )
                .bind(&uid)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

                Ok(Some(UserDetail {
                    id: uid,
                    username: r.get(1),
                    email: r.get(2),
                    display_name: r.get(3),
                    tenant_id: r.get(4),
                    tenant_name: r.get(5),
                    roles,
                    permissions,
                }))
            },
            None => Ok(None),
        }
    }

    async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError> {
        sqlx::query("INSERT INTO user_role (user_id, role_id) VALUES (?1, ?2)")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn list_all(
        &self,
        tenant_id: Option<&str>,
        search: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<UserListInfo>, BackendError> {
        // 使用参数化查询构建 WHERE 条件，防止 SQL 注入
        let mut sql = String::from(
            "SELECT u.id, u.username, u.email, u.display_name, u.tenant_id, \
             t.name as tenant_name, u.status, u.created_at \
             FROM \"user\" u \
             JOIN tenant t ON u.tenant_id = t.id \
             WHERE u.status != 'deleted'",
        );

        if tenant_id.is_some() {
            sql.push_str(" AND u.tenant_id = ?1");
        }
        if search.is_some() {
            sql.push_str(" AND (u.username LIKE ?2 OR u.email LIKE ?2 OR u.display_name LIKE ?2)");
        }
        if status.is_some() {
            sql.push_str(" AND u.status = ?3");
        }
        sql.push_str(" ORDER BY u.created_at DESC");

        let like_pattern = search.map(|s| format!("%{s}%"));
        let mut query = sqlx::query(&sql);

        if let Some(tid) = tenant_id {
            query = query.bind(tid);
        }
        if let Some(ref pattern) = like_pattern {
            query = query.bind(pattern);
        }
        if let Some(st) = status {
            query = query.bind(st);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;

        // 批量查询角色（避免 N+1）
        let user_ids: Vec<String> = rows
            .iter()
            .map(|r: &sqlx::sqlite::SqliteRow| r.get(0))
            .collect();
        let roles_map = self.batch_get_roles(&user_ids).await?;

        let mut users = Vec::new();
        for r in &rows {
            let uid: String = r.get(0);
            let created_at_str: String = r.get(7);
            let created_at = NaiveDateTime::parse_from_str(&created_at_str, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc().to_rfc3339())
                .unwrap_or_else(|_| created_at_str);

            users.push(UserListInfo {
                id: uid.clone(),
                username: r.get(1),
                email: r.get(2),
                display_name: r.get(3),
                tenant_id: r.get(4),
                tenant_name: r.get(5),
                roles: roles_map.get(&uid).cloned().unwrap_or_default(),
                status: r.get(6),
                created_at,
            });
        }
        Ok(users)
    }

    async fn update(&self, user_id: &str, params: &UpdateUserParams) -> Result<(), BackendError> {
        if let Some(ref display_name) = params.display_name {
            sqlx::query("UPDATE \"user\" SET display_name = ?1 WHERE id = ?2")
                .bind(display_name)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        if let Some(ref status) = params.status {
            sqlx::query("UPDATE \"user\" SET status = ?1 WHERE id = ?2")
                .bind(status)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        Ok(())
    }

    async fn soft_delete(&self, user_id: &str) -> Result<(), BackendError> {
        sqlx::query("UPDATE \"user\" SET status = 'deleted' WHERE id = ?1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn reset_password(&self, user_id: &str, password_hash: &str) -> Result<(), BackendError> {
        sqlx::query("UPDATE \"user\" SET password_hash = ?1 WHERE id = ?2")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn remove_roles(&self, user_id: &str) -> Result<(), BackendError> {
        sqlx::query("DELETE FROM user_role WHERE user_id = ?1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}
