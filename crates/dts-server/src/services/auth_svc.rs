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

use anyhow::anyhow;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use dts_backend::repo::{RoleRepo, TenantRepo, UserRepo};
use dts_backend::types::UserRecord;
use sa_token_core::{SaTokenError, SaTokenManager, TokenValue};
use serde::{Deserialize, Serialize};

use crate::models::auth::*;
use crate::models::error::AppError;
use crate::services::sa_auth::LOGIN_TYPE;

#[derive(Serialize, Deserialize)]
struct SessionMetadata {
    #[serde(flatten)]
    identity: AuthIdentity,
    credential_version: String,
}

#[derive(Clone)]
pub struct AuthService {
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub manager: Arc<SaTokenManager>,
}

impl AuthService {
    pub fn new(
        tenant_repo: Arc<dyn TenantRepo>,
        user_repo: Arc<dyn UserRepo>,
        role_repo: Arc<dyn RoleRepo>,
        manager: Arc<SaTokenManager>,
    ) -> Self {
        Self {
            tenant_repo,
            user_repo,
            role_repo,
            manager,
        }
    }

    pub async fn login(&self, req: &LoginRequest) -> Result<LoginResponse, AppError> {
        let tenant = self
            .tenant_repo
            .find_by_slug(&req.tenant_slug)
            .await
            .map_err(|_| AppError::Internal("认证数据读取失败".into()))?
            .filter(|tenant| tenant.status == "active")
            .ok_or_else(|| AppError::Unauthorized("用户名、密码或租户错误".into()))?;
        let user = self
            .user_repo
            .find_by_tenant_and_username(&tenant.id, &req.username)
            .await
            .map_err(|_| AppError::Internal("认证数据读取失败".into()))?
            .filter(|user| user.status == "active")
            .ok_or_else(|| AppError::Unauthorized("用户名、密码或租户错误".into()))?;
        let parsed = PasswordHash::new(&user.password_hash)
            .map_err(|_| AppError::Internal("密码验证失败".into()))?;
        Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed)
            .map_err(|error| match error {
                argon2::password_hash::Error::Password => {
                    AppError::Unauthorized("用户名、密码或租户错误".into())
                },
                _ => AppError::Internal("密码验证失败".into()),
            })?;
        let roles = self
            .manager
            .get_roles_with_type(LOGIN_TYPE, &user.id)
            .await
            .map_err(|_| AppError::Internal("角色数据读取失败".into()))?;
        let permissions = self
            .manager
            .get_permissions_with_type(LOGIN_TYPE, &user.id)
            .await
            .map_err(|_| AppError::Internal("权限数据读取失败".into()))?;
        let token = self.issue_session(&user).await?;
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

    /// Trusted internal issuer for seeded accounts and server-side flows, never an HTTP endpoint.
    pub async fn issue_session_for_user(&self, user_id: &str) -> Result<String, AppError> {
        let user = self.active_user(user_id).await?;
        self.issue_session(&user).await
    }

    async fn issue_session(&self, user: &UserRecord) -> Result<String, AppError> {
        let metadata = SessionMetadata {
            identity: identity(user),
            credential_version: credential_version(&user.password_hash),
        };
        let metadata = serde_json::to_value(metadata)
            .map_err(|_| AppError::Internal("会话元数据编码失败".into()))?;
        self.manager
            .login_with_options(
                user.id.clone(),
                Some(LOGIN_TYPE.into()),
                None,
                Some(metadata),
                None,
                None,
            )
            .await
            .map(|token| token.as_str().to_owned())
            .map_err(|_| AppError::Internal("会话创建失败".into()))
    }

    pub async fn authenticate(&self, token: &str) -> Result<AuthIdentity, AppError> {
        let info = self
            .manager
            .get_token_info(&TokenValue::new(token))
            .await
            .map_err(authentication_error)?;
        if info.login_type.as_ref() != LOGIN_TYPE {
            return Err(invalid_session());
        }
        let metadata: SessionMetadata = info
            .extra_data
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or_else(invalid_session)?;
        let user = self.active_user(info.login_id.as_ref()).await?;
        let current = identity(&user);
        if metadata.identity != current
            || metadata.credential_version != credential_version(&user.password_hash)
        {
            return Err(invalid_session());
        }
        Ok(current)
    }

    pub(super) async fn active_user(&self, user_id: &str) -> Result<UserRecord, AppError> {
        let detail = self
            .user_repo
            .find_detail_by_id(user_id)
            .await?
            .ok_or_else(invalid_session)?;
        self.tenant_repo
            .find_by_id(&detail.tenant_id)
            .await?
            .filter(|tenant| tenant.status == "active")
            .ok_or_else(invalid_session)?;
        self.user_repo
            .find_by_tenant_and_username(&detail.tenant_id, &detail.username)
            .await?
            .filter(|user| {
                user.id == user_id && user.tenant_id == detail.tenant_id && user.status == "active"
            })
            .ok_or_else(invalid_session)
    }

    pub async fn validate_identity(&self, expected: &AuthIdentity) -> Result<(), AppError> {
        let user = self.active_user(&expected.sub).await?;
        if &identity(&user) != expected {
            return Err(invalid_session());
        }
        Ok(())
    }

    pub async fn authorize(
        &self,
        identity: &AuthIdentity,
        permission: &str,
    ) -> Result<(), AppError> {
        self.manager
            .authz_service()
            .check_permission(LOGIN_TYPE, &identity.sub, permission)
            .await
            .map_err(|error| match error {
                SaTokenError::PermissionDenied | SaTokenError::PermissionDeniedDetail(_) => {
                    AppError::Forbidden(format!("缺少权限: {permission}"))
                },
                _ => AppError::Internal("权限校验失败".into()),
            })
    }

    pub async fn logout(&self, token: &str) -> Result<(), AppError> {
        self.authenticate(token).await?;
        self.manager
            .logout(&TokenValue::new(token))
            .await
            .map_err(|_| AppError::Internal("会话退出失败".into()))
    }

    pub async fn revoke_user(&self, user_id: &str) -> Result<(), AppError> {
        let tokens = self
            .manager
            .get_all_tokens_by_login_id(LOGIN_TYPE, user_id)
            .await
            .map_err(|_| AppError::Internal("用户会话读取失败".into()))?;
        // The upstream bulk logout logs and suppresses individual failures; propagate them here.
        for token in tokens {
            self.manager
                .logout(&token)
                .await
                .map_err(|_| AppError::Internal("用户会话撤销失败".into()))?;
        }
        Ok(())
    }

    pub async fn ensure_user_in_tenant(
        &self,
        identity: &AuthIdentity,
        user_id: &str,
    ) -> Result<(), AppError> {
        self.user_repo
            .find_detail_by_id(user_id)
            .await?
            .filter(|user| user.tenant_id == identity.tenant_id)
            .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
        Ok(())
    }

    /// Account changes must not let a delegated administrator take over a more privileged user.
    pub async fn ensure_manageable_user(
        &self,
        identity: &AuthIdentity,
        user_id: &str,
    ) -> Result<UserRecord, AppError> {
        let detail = self
            .user_repo
            .find_detail_by_id(user_id)
            .await?
            .filter(|user| user.tenant_id == identity.tenant_id)
            .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
        let user = self
            .user_repo
            .find_by_tenant_and_username(&detail.tenant_id, &detail.username)
            .await?
            .filter(|user| user.id == user_id)
            .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;
        let granted = self.permissions(identity).await?;
        let target = self
            .manager
            .get_permissions_with_type(LOGIN_TYPE, user_id)
            .await
            .map_err(|_| AppError::Internal("目标用户权限读取失败".into()))?;
        if target
            .iter()
            .any(|permission| !granted.contains(permission))
        {
            return Err(AppError::Forbidden("不能修改权限高于自己的用户".into()));
        }
        Ok(user)
    }

    pub async fn tenant_role(
        &self,
        identity: &AuthIdentity,
        role_id: &str,
    ) -> Result<dts_backend::types::RoleListInfo, AppError> {
        self.role_repo
            .list_all(Some(&identity.tenant_id))
            .await?
            .into_iter()
            .find(|role| role.id == role_id)
            .ok_or_else(|| AppError::NotFound("角色不存在".into()))
    }

    pub async fn ensure_tenant_roles(
        &self,
        identity: &AuthIdentity,
        role_ids: &[String],
    ) -> Result<(), AppError> {
        let roles = self.role_repo.list_all(Some(&identity.tenant_id)).await?;
        if role_ids
            .iter()
            .any(|id| !roles.iter().any(|role| role.id == *id))
        {
            return Err(AppError::BadRequest("角色不存在或不属于当前租户".into()));
        }
        let granted = self.permissions(identity).await?;
        if roles
            .iter()
            .filter(|role| role_ids.contains(&role.id))
            .any(|role| {
                role.permissions
                    .iter()
                    .any(|permission| !granted.contains(permission))
            })
        {
            return Err(AppError::Forbidden("不能授予自己未持有的权限".into()));
        }
        Ok(())
    }

    pub async fn ensure_grantable_permissions(
        &self,
        identity: &AuthIdentity,
        permissions: &[String],
    ) -> Result<(), AppError> {
        let granted = self.permissions(identity).await?;
        if permissions
            .iter()
            .any(|permission| !granted.contains(permission))
        {
            return Err(AppError::Forbidden("不能授予自己未持有的权限".into()));
        }
        Ok(())
    }

    async fn permissions(&self, identity: &AuthIdentity) -> Result<Vec<String>, AppError> {
        self.manager
            .get_permissions_with_type(LOGIN_TYPE, &identity.sub)
            .await
            .map_err(|_| AppError::Internal("权限数据读取失败".into()))
    }

    pub fn hash_password_sync(password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| anyhow!("密码哈希失败"))?;
        Ok(hash.to_string())
    }
}

fn identity(user: &UserRecord) -> AuthIdentity {
    AuthIdentity {
        sub: user.id.clone(),
        tenant_id: user.tenant_id.clone(),
        username: user.username.clone(),
    }
}

fn credential_version(password_hash: &str) -> String {
    URL_SAFE_NO_PAD
        .encode(ring::digest::digest(&ring::digest::SHA256, password_hash.as_bytes()).as_ref())
}

fn invalid_session() -> AppError {
    AppError::Unauthorized("Token 无效或已过期，账户可能已停用".into())
}

fn authentication_error(error: SaTokenError) -> AppError {
    match error {
        SaTokenError::StorageError(_) | SaTokenError::SerializationError(_) => {
            AppError::Internal("会话存储读取失败".into())
        },
        _ => invalid_session(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_use_distinct_salts_and_verify_with_argon2() {
        let password = "密码123!@#";
        let hash = AuthService::hash_password_sync(password).unwrap();
        let second = AuthService::hash_password_sync(password).unwrap();
        assert_ne!(hash, second);
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        );
        assert!(
            Argon2::default()
                .verify_password(b"wrong", &parsed)
                .is_err()
        );
        assert_ne!(credential_version(&hash), credential_version(&second));
        assert!(!credential_version(&hash).contains(&hash));
    }
}
