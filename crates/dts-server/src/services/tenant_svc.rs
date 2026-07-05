use crate::models::auth::TenantInfo;
use anyhow::anyhow;
use sqlx::PgPool;
use uuid::Uuid;

/// 租户管理服务
#[derive(Clone)]
pub struct TenantService {
    pub pool: PgPool,
}

impl TenantService {
    pub fn new(pool: PgPool) -> Self {
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
            "SELECT id FROM tenant WHERE slug = $1"
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
            "INSERT INTO tenant (id, name, slug) VALUES ($1, $2, $3)"
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
            "INSERT INTO \"user\" (id, tenant_id, username, email, password_hash, display_name, status)
             VALUES ($1, $2, $3, $4, $5, $6, 'active')"
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
            "INSERT INTO user_role (user_id, role_id) VALUES ($1, $2)"
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
            "INSERT INTO role (id, tenant_id, name, description, is_system) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(&role_id)
        .bind(tenant_id)
        .bind(name)
        .bind(description)
        .bind(is_system)
        .execute(&self.pool)
        .await?;

        for perm in permissions {
            sqlx::query(
                "INSERT INTO role_permission (role_id, permission) VALUES ($1, $2)"
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
            "SELECT id, name, slug, status, created_at FROM tenant WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("租户不存在"))?;

        Ok(tenant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use sqlx::PgPool;

    async fn create_test_pool() -> PgPool {
        use sqlx::postgres::PgPoolOptions;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://localhost:5432/dts_test")
            .await
            .expect("Failed to create test pool");

        // Create schema
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tenant (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS role (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                is_system BOOLEAN NOT NULL DEFAULT FALSE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(tenant_id, name)
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS role_permission (
                role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
                permission TEXT NOT NULL,
                PRIMARY KEY (role_id, permission)
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS \"user\" (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
                username TEXT NOT NULL,
                email TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(tenant_id, username),
                UNIQUE(tenant_id, email)
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_role (
                user_id TEXT NOT NULL REFERENCES \"user\"(id) ON DELETE CASCADE,
                role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
                PRIMARY KEY (user_id, role_id)
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    // ========== create_tenant Tests ==========

    #[tokio::test]
    async fn create_tenant_creates_new_tenant_and_returns_id() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        let tenant_id = svc
            .create_tenant(
                "Test Tenant",
                "test-tenant",
                "admin",
                "admin@test.com",
                "hashed_password",
            )
            .await
            .unwrap();

        assert!(!tenant_id.is_empty());

        let name: String = sqlx::query_scalar("SELECT name FROM tenant WHERE id = $1")
            .bind(&tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "Test Tenant");
    }

    #[tokio::test]
    async fn create_tenant_creates_default_roles() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        let tenant_id = svc
            .create_tenant("T", "t", "admin", "a@t.com", "pw")
            .await
            .unwrap();

        let role_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM role WHERE tenant_id = $1"
        )
        .bind(&tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(role_count, 3);

        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM role WHERE tenant_id = $1 ORDER BY name"
        )
        .bind(&tenant_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(roles, vec!["developer", "tenant_admin", "viewer"]);
    }

    #[tokio::test]
    async fn create_tenant_creates_admin_user() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        let tenant_id = svc
            .create_tenant("MyOrg", "my-org", "root", "root@myorg.com", "secret_hash")
            .await
            .unwrap();

        let username: String = sqlx::query_scalar(
            "SELECT username FROM \"user\" WHERE tenant_id = $1"
        )
        .bind(&tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(username, "root");
    }

    #[tokio::test]
    async fn create_tenant_duplicate_slug_fails() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        svc.create_tenant("First", "unique-slug", "admin", "a@t.com", "pw")
            .await
            .unwrap();

        let result = svc
            .create_tenant("Second", "unique-slug", "admin2", "b@t.com", "pw")
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("已存在"));
    }

    // ========== list Tests ==========

    #[tokio::test]
    async fn list_returns_empty_vec_when_no_tenants() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool);

        let tenants = svc.list().await.unwrap();
        assert!(tenants.is_empty());
    }

    #[tokio::test]
    async fn list_returns_all_tenants() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        svc.create_tenant("T1", "t1", "admin", "a1@t.com", "pw")
            .await
            .unwrap();
        svc.create_tenant("T2", "t2", "admin", "a2@t.com", "pw")
            .await
            .unwrap();

        let tenants = svc.list().await.unwrap();
        assert_eq!(tenants.len(), 2);

        let slugs: Vec<&str> = tenants.iter().map(|t| t.slug.as_str()).collect();
        assert!(slugs.contains(&"t1"));
        assert!(slugs.contains(&"t2"));
    }

    #[tokio::test]
    async fn list_returns_active_status_by_default() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        svc.create_tenant("Active Corp", "active-corp", "admin", "a@ac.com", "pw")
            .await
            .unwrap();

        let tenants = svc.list().await.unwrap();
        assert_eq!(tenants[0].status, "active");
    }

    // ========== get_by_id Tests ==========

    #[tokio::test]
    async fn get_by_id_returns_tenant_when_found() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool.clone());

        let tenant_id = svc
            .create_tenant("FindMe", "find-me", "admin", "a@fm.com", "pw")
            .await
            .unwrap();

        let tenant = svc.get_by_id(&tenant_id).await.unwrap();
        assert_eq!(tenant.id, tenant_id);
        assert_eq!(tenant.name, "FindMe");
        assert_eq!(tenant.slug, "find-me");
    }

    #[tokio::test]
    async fn get_by_id_non_existent_returns_error() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool);

        let result = svc.get_by_id("non-existent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("不存在"));
    }

    #[tokio::test]
    async fn get_by_id_empty_string_returns_error() {
        let pool = create_test_pool().await;
        let svc = TenantService::new(pool);

        let result = svc.get_by_id("").await;
        assert!(result.is_err());
    }
}
