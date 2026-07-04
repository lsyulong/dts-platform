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
