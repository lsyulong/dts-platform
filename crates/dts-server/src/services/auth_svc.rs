use crate::config::AppConfig;
use crate::models::auth::*;
use anyhow::{anyhow, Context};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sqlx::{Row, SqlitePool};

/// 认证服务 — 处理登录、JWT签发/验证、密码哈希
#[derive(Clone)]
pub struct AuthService {
    pub pool: SqlitePool,
    pub config: AppConfig,
}

impl AuthService {
    pub fn new(pool: SqlitePool, config: AppConfig) -> Self {
        Self { pool, config }
    }

    /// 用户登录
    pub async fn login(&self, req: &LoginRequest) -> anyhow::Result<LoginResponse> {
        // 1. 查找租户
        let tenant = sqlx::query(
            "SELECT id, name, status FROM tenant WHERE slug = ? AND status = 'active'"
        )
        .bind(&req.tenant_slug)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("租户不存在或已停用"))?;

        let tenant_id: String = tenant.get(0);
        let tenant_name: String = tenant.get(1);

        // 2. 查找用户
        let user = sqlx::query(
            "SELECT id, username, email, password_hash, display_name, status
             FROM user WHERE tenant_id = ? AND username = ?"
        )
        .bind(&tenant_id)
        .bind(&req.username)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("用户名或密码错误"))?;

        let user_id: String = user.get(0);
        let username: String = user.get(1);
        let email: String = user.get(2);
        let password_hash: String = user.get(3);
        let display_name: Option<String> = user.get(4);
        let user_status: String = user.get(5);

        if user_status != "active" {
            return Err(anyhow!("账户已被禁用"));
        }

        // 3. 验证密码
        let parsed_hash = PasswordHash::new(&password_hash)
            .map_err(|_| anyhow!("内部错误: 密码哈希格式无效"))?;
        Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .map_err(|_| anyhow!("用户名或密码错误"))?;

        // 4. 获取角色列表
        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT r.name FROM role r
             INNER JOIN user_role ur ON r.id = ur.role_id
             WHERE ur.user_id = ?"
        )
        .bind(&user_id)
        .fetch_all(&self.pool)
        .await?;

        // 5. 获取权限列表
        let permissions: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT rp.permission FROM role_permission rp
             INNER JOIN user_role ur ON rp.role_id = ur.role_id
             WHERE ur.user_id = ?"
        )
        .bind(&user_id)
        .fetch_all(&self.pool)
        .await?;

        // 6. 生成 JWT
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.clone(),
            tenant_id: tenant_id.clone(),
            username: username.clone(),
            exp: (now + chrono::Duration::hours(self.config.jwt_expiry_hours)).timestamp()
                as usize,
            iat: now.timestamp() as usize,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .context("JWT 签发失败")?;

        Ok(LoginResponse {
            token,
            user: UserInfo {
                id: user_id,
                username,
                email,
                display_name,
                tenant_id,
                tenant_name,
                roles,
                permissions,
            },
        })
    }

    /// 验签 JWT，返回 Claims
    pub fn verify_token(&self, token: &str) -> anyhow::Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .context("Token 验证失败")?;
        Ok(token_data.claims)
    }

    /// 哈希密码 (同步方法，用于初始化种子数据)
    pub fn hash_password_sync(password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {}", e))?;
        Ok(hash.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use argon2::PasswordVerifier;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use pretty_assertions::assert_eq;
    use sqlx::SqlitePool;

    fn test_config() -> AppConfig {
        AppConfig::for_test()
    }

    async fn create_test_pool() -> SqlitePool {
        use sqlx::sqlite::SqlitePoolOptions;
        SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool")
    }

    /// Helper: generate a valid JWT for verify_token tests
    fn generate_token(sub: &str, tenant_id: &str, secret: &str, hours_valid: i64) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: sub.into(),
            tenant_id: tenant_id.into(),
            username: "testuser".into(),
            exp: (now + chrono::Duration::hours(hours_valid)).timestamp() as usize,
            iat: now.timestamp() as usize,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    /// Helper: generate an expired JWT
    fn generate_expired_token(secret: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: "user-1".into(),
            tenant_id: "tenant-1".into(),
            username: "testuser".into(),
            exp: (now - chrono::Duration::hours(1)).timestamp() as usize,
            iat: (now - chrono::Duration::hours(2)).timestamp() as usize,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    // ========== hash_password_sync Tests ==========

    #[test]
    fn hash_password_sync_produces_valid_argon2_hash() {
        let hash = AuthService::hash_password_sync("mypassword").unwrap();
        // Argon2 hashes start with $argon2
        assert!(hash.starts_with("$argon2"));
        // Should contain version and parameters
        assert!(hash.contains("$v="));
        assert!(hash.contains("$m="));
    }

    #[test]
    fn hash_password_sync_produces_different_hashes_for_same_password() {
        let hash1 = AuthService::hash_password_sync("samepassword").unwrap();
        let hash2 = AuthService::hash_password_sync("samepassword").unwrap();
        // Different salts should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_password_sync_generated_hash_can_be_verified() {
        let password = "secret123";
        let hash = AuthService::hash_password_sync(password).unwrap();

        let parsed_hash = PasswordHash::new(&hash).unwrap();
        let result = Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash);
        assert!(result.is_ok());
    }

    #[test]
    fn hash_password_sync_generated_hash_fails_with_wrong_password() {
        let hash = AuthService::hash_password_sync("correct_password").unwrap();

        let parsed_hash = PasswordHash::new(&hash).unwrap();
        let result = Argon2::default()
            .verify_password("wrong_password".as_bytes(), &parsed_hash);
        assert!(result.is_err());
    }

    #[test]
    fn hash_password_sync_empty_password_works() {
        let hash = AuthService::hash_password_sync("").unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn hash_password_sync_long_password_works() {
        let long_pw = "a".repeat(1000);
        let hash = AuthService::hash_password_sync(&long_pw).unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn hash_password_sync_unicode_password_works() {
        let hash = AuthService::hash_password_sync("密码123!@#").unwrap();
        assert!(hash.starts_with("$argon2"));
    }

    // ========== verify_token Tests ==========

    #[tokio::test]
    async fn verify_token_valid_token_returns_claims() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        let token = generate_token("user-1", "tenant-1", &svc.config.jwt_secret, 1);
        let result = svc.verify_token(&token);
        assert!(result.is_ok());
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.username, "testuser");
    }

    #[tokio::test]
    async fn verify_token_expired_token_returns_err() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        let token = generate_expired_token(&svc.config.jwt_secret);
        let result = svc.verify_token(&token);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_token_wrong_secret_returns_err() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        let token = generate_token("user-1", "tenant-1", "wrong-secret-key", 1);
        let result = svc.verify_token(&token);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_token_malformed_token_returns_err() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        let result = svc.verify_token("not-a-valid-jwt");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_token_empty_token_returns_err() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        let result = svc.verify_token("");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_token_token_with_three_parts_but_invalid_returns_err() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);

        // Looks like JWT format but has invalid base64 content
        let result = svc.verify_token("header.payload.signature");
        assert!(result.is_err());
    }
}
