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
use dts_backend::{BackendError, repo::UserRepo, repo::user::UserDetail, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgUserRepo {
    pool: PgPool,
}

impl PgUserRepo {
    pub fn new(pool: PgPool) -> Self {
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
            .map(|(i, _)| format!("${}", i + 1))
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
impl UserRepo for PgUserRepo {
    async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO \"user\" (id, tenant_id, username, email, password_hash, display_name, status)
             VALUES ($1, $2, $3, $4, $5, $6, 'active')",
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
             FROM \"user\" WHERE tenant_id = $1 AND username = $2",
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
             WHERE u.id = $1",
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
                     WHERE ur.user_id = $1",
                )
                .bind(&uid)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

                let permissions: Vec<String> = sqlx::query_scalar(
                    "SELECT DISTINCT rp.permission FROM role_permission rp
                     INNER JOIN user_role ur ON rp.role_id = ur.role_id
                     WHERE ur.user_id = $1",
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
        sqlx::query("INSERT INTO user_role (user_id, role_id) VALUES ($1, $2)")
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
        let mut sql = String::from(
            "SELECT u.id, u.username, u.email, u.display_name, u.tenant_id, \
             t.name as tenant_name, u.status, \
             to_char(u.created_at, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') as created_at \
             FROM \"user\" u \
             JOIN tenant t ON u.tenant_id = t.id \
             WHERE u.status != 'deleted'",
        );

        let mut idx: i32 = 0;
        let mut binds: Vec<String> = Vec::new();

        if let Some(tid) = tenant_id {
            idx += 1;
            sql.push_str(&format!(" AND u.tenant_id = ${idx}"));
            binds.push(tid.to_string());
        }
        if let Some(ref s) = search {
            idx += 1;
            let p = idx;
            idx += 1;
            let p2 = idx;
            idx += 1;
            let p3 = idx;
            sql.push_str(&format!(
                " AND (u.username ILIKE ${p} OR u.email ILIKE ${p2} OR u.display_name ILIKE ${p3})",
            ));
            let pattern = format!("%{s}%");
            binds.push(pattern.clone());
            binds.push(pattern.clone());
            binds.push(pattern);
        }
        if let Some(st) = status {
            idx += 1;
            sql.push_str(&format!(" AND u.status = ${idx}"));
            binds.push(st.to_string());
        }
        sql.push_str(" ORDER BY u.created_at DESC");

        let mut query = sqlx::query(&sql);
        for b in &binds {
            query = query.bind(b);
        }
        let rows = query.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;

        // 批量查询角色（避免 N+1）
        let user_ids: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
        let roles_map = self.batch_get_roles(&user_ids).await?;

        let mut users = Vec::new();
        for r in &rows {
            let uid: String = r.get(0);
            users.push(UserListInfo {
                id: uid.clone(),
                username: r.get(1),
                email: r.get(2),
                display_name: r.get(3),
                tenant_id: r.get(4),
                tenant_name: r.get(5),
                roles: roles_map.get(&uid).cloned().unwrap_or_default(),
                status: r.get(6),
                created_at: r.get(7),
            });
        }
        Ok(users)
    }

    async fn update(&self, user_id: &str, params: &UpdateUserParams) -> Result<(), BackendError> {
        let mut transaction = self.pool.begin().await.map_err(map_sqlx_error)?;
        if let Some(ref display_name) = params.display_name {
            sqlx::query("UPDATE \"user\" SET display_name = $1 WHERE id = $2")
                .bind(display_name)
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(map_sqlx_error)?;
        }
        if let Some(ref status) = params.status {
            sqlx::query("UPDATE \"user\" SET status = $1 WHERE id = $2")
                .bind(status)
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(super::product_resource::map_reference_error)?;
        }
        transaction.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn soft_delete(&self, user_id: &str) -> Result<(), BackendError> {
        sqlx::query("UPDATE \"user\" SET status = 'deleted' WHERE id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(super::product_resource::map_reference_error)?;
        Ok(())
    }

    async fn reset_password(&self, user_id: &str, password_hash: &str) -> Result<(), BackendError> {
        sqlx::query("UPDATE \"user\" SET password_hash = $1 WHERE id = $2")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn remove_roles(&self, user_id: &str) -> Result<(), BackendError> {
        sqlx::query("DELETE FROM user_role WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn list_references(
        &self,
        tenant_id: &str,
        search: &str,
        limit: u32,
        offset: u32,
    ) -> Result<dts_backend::repo::user::UserReferencePage, BackendError> {
        use dts_backend::repo::user::{UserReferenceItem, UserReferencePage};
        if !(1..=100).contains(&limit) || search.len() > 128 || search.chars().any(char::is_control)
        {
            return Err(BackendError::ConstraintViolation(
                "用户目录筛选或分页无效".into(),
            ));
        }
        let pattern = format!(
            "%{}%",
            search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"user\" WHERE tenant_id=$1 AND status='active' AND (username ILIKE $2 ESCAPE '\\' OR COALESCE(display_name,'') ILIKE $2 ESCAPE '\\')")
            .bind(tenant_id).bind(&pattern).fetch_one(&mut *tx).await.map_err(map_sqlx_error)?;
        let rows = sqlx::query("SELECT id,tenant_id,username,display_name FROM \"user\" WHERE tenant_id=$1 AND status='active' AND (username ILIKE $2 ESCAPE '\\' OR COALESCE(display_name,'') ILIKE $2 ESCAPE '\\') ORDER BY username,id LIMIT $3 OFFSET $4")
            .bind(tenant_id).bind(&pattern).bind(i64::from(limit)).bind(i64::from(offset)).fetch_all(&mut *tx).await.map_err(map_sqlx_error)?;
        let data = rows
            .into_iter()
            .map(|row| {
                Ok(UserReferenceItem {
                    id: row.try_get("id").map_err(map_sqlx_error)?,
                    tenant_id: row.try_get("tenant_id").map_err(map_sqlx_error)?,
                    username: row.try_get("username").map_err(map_sqlx_error)?,
                    display_name: row.try_get("display_name").map_err(map_sqlx_error)?,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(UserReferencePage { data, total })
    }

    async fn update_self_profile(
        &self,
        user_id: &str,
        tenant_id: &str,
        email: &str,
        display_name: &str,
    ) -> Result<bool, BackendError> {
        let result = sqlx::query("UPDATE \"user\" SET email = $1, display_name = $2, updated_at = NOW() WHERE id = $3 AND tenant_id = $4 AND status = 'active' AND EXISTS(SELECT 1 FROM tenant t WHERE t.id = $4 AND t.status = 'active')")
            .bind(email).bind(display_name).bind(user_id).bind(tenant_id)
            .execute(&self.pool).await.map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn compare_exchange_password(
        &self,
        user_id: &str,
        tenant_id: &str,
        expected_hash: &str,
        new_hash: &str,
    ) -> Result<bool, BackendError> {
        let result = sqlx::query("UPDATE \"user\" SET password_hash = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3 AND password_hash = $4 AND status = 'active' AND EXISTS(SELECT 1 FROM tenant t WHERE t.id = $3 AND t.status = 'active')")
            .bind(new_hash).bind(user_id).bind(tenant_id).bind(expected_hash)
            .execute(&self.pool).await.map_err(map_sqlx_error)?;
        Ok(result.rows_affected() == 1)
    }
}
