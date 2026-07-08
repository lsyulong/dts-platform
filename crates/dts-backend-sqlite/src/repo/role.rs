use async_trait::async_trait;
use dts_backend::{
    BackendError, repo::RoleRepo, repo::role::DefaultRoles, types::CreateRoleParams,
};
use dts_backend_rdbms::map_sqlx_error;
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
                permissions: vec![
                    "task:read".into(),
                    "task:write".into(),
                    "task:execute".into(),
                    "datasource:read".into(),
                    "datasource:write".into(),
                    "monitor:read".into(),
                    "alert:read".into(),
                    "alert:write".into(),
                    "user:manage".into(),
                    "audit:read".into(),
                ],
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
}
