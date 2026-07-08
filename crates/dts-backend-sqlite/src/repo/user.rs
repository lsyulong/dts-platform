use async_trait::async_trait;
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
}
