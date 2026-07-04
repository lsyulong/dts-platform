use crate::models::auth::TenantInfo;
use anyhow::anyhow;
use sqlx::SqlitePool;
use uuid::Uuid;

/// 租户管理服务
#[derive(Clone)]
pub struct TenantService {
    pub pool: SqlitePool,
}

impl TenantService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建租户（同时创建默认角色和 admin 用户）
    pub async fn create_tenant(
        &self,
        name: &str,
        slug: &str,
        admin_username: &str,
        admin_email: &str,
        admin_password_hash: &str,
    ) -> anyhow::Result<String> {
        // 检查 slug 唯一性
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM tenant WHERE slug = ?"
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Err(anyhow!("租户标识 '{}' 已存在", slug));
        }

        let tenant_id = Uuid::new_v4().to_string();

        // 创建租户
        sqlx::query(
            "INSERT INTO tenant (id, name, slug) VALUES (?, ?, ?)"
        )
        .bind(&tenant_id)
        .bind(name)
        .bind(slug)
        .execute(&self.pool)
        .await?;

        // 创建默认角色: tenant_admin, developer, viewer
        let admin_role_id = self
            .create_role(
                &tenant_id,
                "tenant_admin",
                "租户管理员",
                &["task:read","task:write","task:execute","datasource:read","datasource:write","monitor:read","alert:read","alert:write","user:manage","audit:read"],
                true,
            )
            .await?;

        self.create_role(
            &tenant_id,
            "developer",
            "开发运维",
            &["task:read","task:write","task:execute","datasource:read","datasource:write","monitor:read","alert:read"],
            true,
        )
        .await?;

        self.create_role(
            &tenant_id,
            "viewer",
            "只读用户",
            &["task:read","datasource:read","monitor:read","alert:read"],
            true,
        )
        .await?;

        // 创建管理员用户
        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO user (id, tenant_id, username, email, password_hash, display_name, status)
             VALUES (?, ?, ?, ?, ?, ?, 'active')"
        )
        .bind(&user_id)
        .bind(&tenant_id)
        .bind(admin_username)
        .bind(admin_email)
        .bind(admin_password_hash)
        .bind(format!("{}管理员", name))
        .execute(&self.pool)
        .await?;

        // 分配 admin 角色
        sqlx::query(
            "INSERT INTO user_role (user_id, role_id) VALUES (?, ?)"
        )
        .bind(&user_id)
        .bind(&admin_role_id)
        .execute(&self.pool)
        .await?;

        log::info!(
            "租户 '{}' 创建成功, 管理员: {}",
            name,
            admin_username
        );

        Ok(tenant_id)
    }

    /// 创建角色（内部方法）
    async fn create_role(
        &self,
        tenant_id: &str,
        name: &str,
        description: &str,
        permissions: &[&str],
        is_system: bool,
    ) -> anyhow::Result<String> {
        let role_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO role (id, tenant_id, name, description, is_system) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&role_id)
        .bind(tenant_id)
        .bind(name)
        .bind(description)
        .bind(is_system as i32)
        .execute(&self.pool)
        .await?;

        for perm in permissions {
            sqlx::query(
                "INSERT INTO role_permission (role_id, permission) VALUES (?, ?)"
            )
            .bind(&role_id)
            .bind(*perm)
            .execute(&self.pool)
            .await?;
        }

        Ok(role_id)
    }

    /// 获取租户列表
    pub async fn list(&self) -> anyhow::Result<Vec<TenantInfo>> {
        let rows = sqlx::query_as::<_, TenantInfo>(
            "SELECT id, name, slug, status, created_at FROM tenant ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// 获取单个租户
    pub async fn get_by_id(&self, id: &str) -> anyhow::Result<TenantInfo> {
        let tenant = sqlx::query_as::<_, TenantInfo>(
            "SELECT id, name, slug, status, created_at FROM tenant WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("租户不存在"))?;

        Ok(tenant)
    }
}
