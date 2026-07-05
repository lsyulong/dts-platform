use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

use crate::services::auth_svc::AuthService;

/// 初始化种子数据：默认租户和管理员用户
pub async fn seed_default_data(pool: &PgPool) -> anyhow::Result<()> {
    // 检查是否已有租户
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        log::info!("数据库已有数据，跳过种子数据初始化");
        return Ok(());
    }

    log::info!("初始化默认种子数据...");

    // 创建默认租户
    let tenant_id = Uuid::new_v4().to_string();
    let tenant_name = "默认租户";
    let tenant_slug = "default";

    sqlx::query("INSERT INTO tenant (id, name, slug) VALUES ($1, $2, $3)")
        .bind(&tenant_id)
        .bind(tenant_name)
        .bind(tenant_slug)
        .execute(pool)
        .await?;

    // 创建角色
    let admin_role_id = create_role(
        pool,
        &tenant_id,
        "tenant_admin",
        "租户管理员",
        &[
            "task:read",
            "task:write",
            "task:execute",
            "datasource:read",
            "datasource:write",
            "monitor:read",
            "alert:read",
            "alert:write",
            "user:manage",
            "audit:read",
        ],
        true,
    )
    .await?;

    create_role(
        pool,
        &tenant_id,
        "developer",
        "开发运维",
        &[
            "task:read",
            "task:write",
            "task:execute",
            "datasource:read",
            "datasource:write",
            "monitor:read",
            "alert:read",
        ],
        true,
    )
    .await?;

    create_role(
        pool,
        &tenant_id,
        "viewer",
        "只读用户",
        &["task:read", "datasource:read", "monitor:read", "alert:read"],
        true,
    )
    .await?;

    // 创建管理员用户
    let admin_password_hash =
        AuthService::hash_password_sync("admin123").context("密码哈希失败")?;

    let user_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO \"user\" (id, tenant_id, username, email, password_hash, display_name)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .bind("admin")
    .bind("admin@dts.local")
    .bind(&admin_password_hash)
    .bind("系统管理员")
    .execute(pool)
    .await?;

    sqlx::query("INSERT INTO user_role (user_id, role_id) VALUES ($1, $2)")
        .bind(&user_id)
        .bind(&admin_role_id)
        .execute(pool)
        .await?;

    // 创建演示数据源
    let mysql_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, username, encrypted_password, database_name, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind(&mysql_id)
    .bind(&tenant_id)
    .bind("生产 MySQL")
    .bind("MySQL")
    .bind("192.168.1.100")
    .bind(3306i64)
    .bind("root")
    .bind("enc:demo123")
    .bind("ecommerce")
    .bind(&user_id)
    .execute(pool)
    .await?;

    let pg_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, username, encrypted_password, database_name, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    )
    .bind(&pg_id)
    .bind(&tenant_id)
    .bind("分析 PostgreSQL")
    .bind("PostgreSQL")
    .bind("192.168.1.101")
    .bind(5432i64)
    .bind("analyst")
    .bind("enc:demo456")
    .bind("warehouse")
    .bind(&user_id)
    .execute(pool)
    .await?;

    log::info!(
        "种子数据初始化完成: 租户='{}', 管理员='admin', 密码='admin123'",
        tenant_name
    );

    Ok(())
}

async fn create_role(
    pool: &PgPool,
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
    .execute(pool)
    .await?;

    for perm in permissions {
        sqlx::query("INSERT INTO role_permission (role_id, permission) VALUES ($1, $2)")
            .bind(&role_id)
            .bind(*perm)
            .execute(pool)
            .await?;
    }

    Ok(role_id)
}
