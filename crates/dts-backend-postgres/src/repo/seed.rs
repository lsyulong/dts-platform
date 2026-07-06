use async_trait::async_trait;
use dts_backend::{BackendError, repo::SeedRepo};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;

use crate::repo::{
    PgDatasourceRepo, PgRoleRepo, PgTenantRepo, PgUserRepo,
};
use dts_backend::repo::{DatasourceRepo, RoleRepo, TenantRepo, UserRepo};
use dts_backend::types::*;
use uuid::Uuid;

pub struct PgSeedRepo {
    pool: PgPool,
}

impl PgSeedRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SeedRepo for PgSeedRepo {
    async fn has_data(&self) -> Result<bool, BackendError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(count > 0)
    }

    async fn seed(&self) -> Result<(), BackendError> {
        log::info!("初始化默认种子数据...");

        let tenant_repo = PgTenantRepo::new(self.pool.clone());
        let role_repo = PgRoleRepo::new(self.pool.clone());
        let user_repo = PgUserRepo::new(self.pool.clone());
        let ds_repo = PgDatasourceRepo::new(self.pool.clone());

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
        let admin_password_hash = hash_password_sync("admin123")?;
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

        user_repo.assign_role(&user_id, &roles.admin_role_id).await?;

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

        log::info!("种子数据初始化完成: 租户='{tenant_name}', 管理员='admin', 密码='admin123'");
        Ok(())
    }
}

/// 使用 Argon2 哈希密码（同步方法，用于种子数据初始化）
fn hash_password_sync(password: &str) -> Result<String, BackendError> {
    use argon2::{
        PasswordHasher,
        password_hash::{SaltString, rand_core::OsRng},
    };
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| BackendError::Internal(format!("密码哈希失败: {e}")))?;
    Ok(hash.to_string())
}
