//! 集成测试共享工具模块
//!
//! 提供测试数据库初始化、HTTP 测试客户端、JWT token 生成等公共函数。

use dts_server::config::AppConfig;
use dts_server::db;
use dts_server::models::auth::Claims;
use jsonwebtoken::{EncodingKey, Header};
use sqlx::PgPool;

/// 创建测试用数据库连接并运行迁移
pub async fn setup_test_db() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost:5432/dts_test".to_string());
    let pool = db::create_pool(&database_url).await?;
    db::run_migrations(&pool).await?;
    Ok(pool)
}

/// 创建测试用数据库（含种子数据）
pub async fn setup_test_db_with_seed() -> anyhow::Result<PgPool> {
    let pool = setup_test_db().await?;
    db::seed_default_data(&pool).await?;
    Ok(pool)
}

/// 创建测试用 AppConfig（使用固定的 JWT 密钥便于测试）
pub fn test_config() -> AppConfig {
    AppConfig::new(
        std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost:5432/dts_test".to_string()),
        "test-jwt-secret-for-integration-tests",
        24,
        "127.0.0.1",
        8080,
        "/tmp/dts-test-data",
    )
}

/// 生成测试用 JWT Token
pub fn generate_test_token(
    secret: &str,
    user_id: &str,
    tenant_id: &str,
    username: &str,
) -> String {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        tenant_id: tenant_id.to_string(),
        username: username.to_string(),
        exp: (now + chrono::Duration::hours(24)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("测试 Token 生成失败")
}

/// 获取默认租户的管理员 token（假设 seed 数据已初始化）
pub async fn get_admin_token(pool: &PgPool) -> String {
    let config = test_config();

    // 从数据库查询默认租户和 admin 用户
    let row = sqlx::query(
        "SELECT t.id as tenant_id, u.id as user_id, u.username
         FROM tenant t
         JOIN \"user\" u ON u.tenant_id = t.id
         WHERE t.slug = 'default' AND u.username = 'admin'"
    )
    .fetch_one(pool)
    .await
    .expect("种子数据应存在");

    let tenant_id: String = row.get(0);
    let user_id: String = row.get(1);
    let username: String = row.get(2);

    generate_test_token(&config.jwt_secret, &user_id, &tenant_id, &username)
}
