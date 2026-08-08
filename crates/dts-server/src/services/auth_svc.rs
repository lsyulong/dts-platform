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


use std::sync::Arc;

use crate::models::auth::*;
use crate::services::token::{self, Claims};
use anyhow::{Context, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use dts_backend::repo::{RoleRepo, TenantRepo, UserRepo};

/// 认证服务 — 处理登录、JWT签发/验证、密码哈希
#[derive(Clone)]
pub struct AuthService {
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
}

impl AuthService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepo>,
        user_repo: Arc<dyn UserRepo>,
        role_repo: Arc<dyn RoleRepo>,
        jwt_secret: String,
        jwt_expiry_hours: i64,
    ) -> Self {
        Self {
            tenant_repo,
            user_repo,
            role_repo,
            jwt_secret,
            jwt_expiry_hours,
        }
    }

    /// 用户登录
    pub async fn login(&self, req: &LoginRequest) -> anyhow::Result<LoginResponse> {
        // 1. 查找租户
        let tenant = self
            .tenant_repo
            .find_by_slug(&req.tenant_slug)
            .await?
            .ok_or_else(|| anyhow!("租户不存在或已停用"))?;

        if tenant.status != "active" {
            return Err(anyhow!("租户不存在或已停用"));
        }

        // 2. 查找用户
        let user = self
            .user_repo
            .find_by_tenant_and_username(&tenant.id, &req.username)
            .await?
            .ok_or_else(|| anyhow!("用户名或密码错误"))?;

        if user.status != "active" {
            return Err(anyhow!("账户已被禁用"));
        }

        // 3. 验证密码
        let parsed_hash = PasswordHash::new(&user.password_hash)
            .map_err(|_| anyhow!("内部错误: 密码哈希格式无效"))?;
        Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .map_err(|_| anyhow!("用户名或密码错误"))?;

        // 4. 获取角色和权限
        let roles = self.role_repo.find_user_role_names(&user.id).await?;
        let permissions = self.role_repo.find_user_permissions(&user.id).await?;

        // 5. 生成 JWT
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user.id.clone(),
            tenant_id: tenant.id.clone(),
            username: user.username.clone(),
            exp: (now + chrono::Duration::hours(self.jwt_expiry_hours)).timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        let token = token::create_token(&claims, &self.jwt_secret).context("JWT 签发失败")?;

        Ok(LoginResponse {
            token,
            user: UserInfo {
                id: user.id,
                username: user.username,
                email: user.email,
                display_name: user.display_name,
                tenant_id: tenant.id,
                tenant_name: tenant.name,
                roles,
                permissions,
            },
        })
    }

    /// 验签 JWT，返回 Claims
    pub fn verify_token(&self, token: &str) -> anyhow::Result<Claims> {
        token::validate_token(token, &self.jwt_secret).context("Token 验证失败")
    }

    /// 哈希密码 (同步方法，用于初始化种子数据)
    pub fn hash_password_sync(password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {e}"))?;
        Ok(hash.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::PasswordVerifier;
    use pretty_assertions::assert_eq;

    // ========== hash_password_sync Tests ==========

    #[test]
    fn hash_password_sync_produces_valid_argon2_hash() {
        let hash = AuthService::hash_password_sync("mypassword").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(hash.contains("$v="));
        assert!(hash.contains("$m="));
    }

    #[test]
    fn hash_password_sync_produces_different_hashes_for_same_password() {
        let hash1 = AuthService::hash_password_sync("samepassword").unwrap();
        let hash2 = AuthService::hash_password_sync("samepassword").unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_password_sync_generated_hash_can_be_verified() {
        let password = "secret123";
        let hash = AuthService::hash_password_sync(password).unwrap();
        let parsed_hash = PasswordHash::new(&hash).unwrap();
        let result = Argon2::default().verify_password(password.as_bytes(), &parsed_hash);
        assert!(result.is_ok());
    }

    #[test]
    fn hash_password_sync_generated_hash_fails_with_wrong_password() {
        let hash = AuthService::hash_password_sync("correct_password").unwrap();
        let parsed_hash = PasswordHash::new(&hash).unwrap();
        let result = Argon2::default().verify_password("wrong_password".as_bytes(), &parsed_hash);
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

    fn generate_token(sub: &str, tenant_id: &str, secret: &str, hours_valid: i64) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: sub.into(),
            tenant_id: tenant_id.into(),
            username: "testuser".into(),
            exp: (now + chrono::Duration::hours(hours_valid)).timestamp() as usize,
            iat: now.timestamp() as usize,
        };
        token::create_token(&claims, secret).unwrap()
    }

    fn generate_expired_token(secret: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: "user-1".into(),
            tenant_id: "tenant-1".into(),
            username: "testuser".into(),
            exp: (now - chrono::Duration::hours(1)).timestamp() as usize,
            iat: (now - chrono::Duration::hours(2)).timestamp() as usize,
        };
        token::create_token(&claims, secret).unwrap()
    }

    #[test]
    fn verify_token_valid_token_returns_claims() {
        let secret = "test-secret";
        let token = generate_token("user-1", "tenant-1", secret, 1);
        let claims = token::validate_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.username, "testuser");
    }

    #[test]
    fn verify_token_expired_token_returns_err() {
        let secret = "test-secret";
        let token = generate_expired_token(secret);
        let result = token::validate_token(&token, secret);
        assert!(result.is_err());
    }

    #[test]
    fn verify_token_wrong_secret_returns_err() {
        let token = generate_token("user-1", "tenant-1", "correct-secret", 1);
        let result = token::validate_token(&token, "wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn verify_token_malformed_token_returns_err() {
        let result = token::validate_token("not-a-valid-jwt", "secret");
        assert!(result.is_err());
    }

    #[test]
    fn verify_token_empty_token_returns_err() {
        let result = token::validate_token("", "secret");
        assert!(result.is_err());
    }
}
