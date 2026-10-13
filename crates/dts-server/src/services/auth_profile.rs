// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use sa_token_core::TokenValue;

use crate::models::{
    auth::{AuthIdentity, ChangePasswordRequest, UpdateProfileRequest, UserInfo},
    error::AppError,
};
use crate::services::auth_svc::AuthService;
use crate::services::sa_auth::LOGIN_TYPE;

impl AuthService {
    pub async fn update_self_profile(
        &self,
        identity: &AuthIdentity,
        request: &UpdateProfileRequest,
    ) -> Result<UserInfo, AppError> {
        self.validate_identity(identity).await?;
        let email = request.email.trim();
        let name = request.display_name.trim();
        validate_profile(email, name)?;
        if !self
            .user_repo
            .update_self_profile(&identity.sub, &identity.tenant_id, email, name)
            .await?
        {
            return Err(AppError::Unauthorized("账户已失效，请重新登录".into()));
        }
        let detail = self
            .user_repo
            .find_detail_by_id(&identity.sub)
            .await?
            .filter(|detail| detail.tenant_id == identity.tenant_id)
            .ok_or_else(|| AppError::Unauthorized("账户已失效，请重新登录".into()))?;
        Ok(UserInfo {
            id: detail.id,
            username: detail.username,
            email: detail.email,
            display_name: detail.display_name,
            tenant_id: detail.tenant_id,
            tenant_name: detail.tenant_name,
            roles: detail.roles,
            permissions: detail.permissions,
        })
    }

    pub async fn change_self_password(
        &self,
        identity: &AuthIdentity,
        request: &ChangePasswordRequest,
    ) -> Result<(), AppError> {
        let user = self.active_user(&identity.sub).await?;
        if user.tenant_id != identity.tenant_id || user.username != identity.username {
            return Err(AppError::Unauthorized("账户身份已失效".into()));
        }
        validate_password_change(request)?;
        let expected_hash = user.password_hash;
        let new_hash = verify_and_hash_password(&expected_hash, request).await?;
        let old_tokens = self
            .manager
            .get_all_tokens_by_login_id(LOGIN_TYPE, &identity.sub)
            .await
            .map_err(|_| AppError::Internal("用户会话读取失败".into()))?;
        if !self
            .user_repo
            .compare_exchange_password(
                &identity.sub,
                &identity.tenant_id,
                &expected_hash,
                &new_hash,
            )
            .await?
        {
            return Err(AppError::BadRequest(
                "密码或账户状态已变更，请重新登录后再试".into(),
            ));
        }
        // Snapshot before commit so a subsequent login with the new password remains valid.
        // Credential fingerprints also reject any old session issued between snapshot and commit.
        self.revoke_password_sessions(&identity.sub, old_tokens)
            .await
    }

    async fn revoke_password_sessions(
        &self,
        user_id: &str,
        tokens: Vec<TokenValue>,
    ) -> Result<(), AppError> {
        let mut cleanup_failed = false;
        for token in tokens {
            if self.manager.logout(&token).await.is_err() {
                cleanup_failed = true;
            }
        }
        if cleanup_failed {
            tracing::error!(
                user_id,
                "password committed; old session storage cleanup failed"
            );
            return Err(AppError::Internal(
                "密码已更新，旧会话存储清理失败，请重新登录".into(),
            ));
        }
        Ok(())
    }
}

fn validate_password_change(request: &ChangePasswordRequest) -> Result<(), AppError> {
    if !(12..=1024).contains(&request.new_password.len()) {
        return Err(AppError::BadRequest(
            "新密码长度必须为 12 到 1024 字节".into(),
        ));
    }
    if request.current_password.len() > 1024 || request.current_password.is_empty() {
        return Err(AppError::BadRequest("当前密码不正确".into()));
    }
    if request.current_password == request.new_password {
        return Err(AppError::BadRequest("新密码必须与当前密码不同".into()));
    }
    Ok(())
}

async fn verify_and_hash_password(
    stored_hash: &str,
    request: &ChangePasswordRequest,
) -> Result<String, AppError> {
    let stored_hash = stored_hash.to_owned();
    let current_password = request.current_password.clone();
    let new_password = request.new_password.clone();
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&stored_hash)
            .map_err(|_| AppError::Internal("密码验证失败".into()))?;
        Argon2::default()
            .verify_password(current_password.as_bytes(), &parsed)
            .map_err(|error| match error {
                argon2::password_hash::Error::Password => {
                    AppError::BadRequest("当前密码不正确".into())
                },
                _ => AppError::Internal("密码验证失败".into()),
            })?;
        AuthService::hash_password_sync(&new_password)
            .map_err(|_| AppError::Internal("密码更新失败".into()))
    })
    .await
    .map_err(|_| AppError::Internal("密码更新任务失败".into()))?
}

fn validate_profile(email: &str, name: &str) -> Result<(), AppError> {
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(AppError::BadRequest("显示名称须为 1 到 120 个字符".into()));
    }
    let valid_email = email.len() <= 254
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        });
    if !valid_email {
        return Err(AppError::BadRequest("邮箱格式不正确".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_actual_profile_fields_without_accepting_control_characters() {
        assert!(validate_profile("admin@default", "管理员").is_ok());
        assert!(validate_profile("a@example.invalid", "A").is_ok());
        for (email, name) in [
            ("no-address", "A"),
            ("a@@domain", "A"),
            ("a@domain", ""),
            ("a@domain", "A\nB"),
        ] {
            assert!(validate_profile(email, name).is_err());
        }
    }
    #[test]
    fn password_request_debug_never_contains_passwords() {
        let request = ChangePasswordRequest {
            current_password: "old-secret".into(),
            new_password: "new-secret-long".into(),
        };
        let text = format!("{request:?}");
        assert!(!text.contains("old-secret"));
        assert!(!text.contains("new-secret-long"));
    }
    #[test]
    fn password_policy_counts_utf8_bytes_and_rejects_same_or_oversized_passwords() {
        let request = |new_password: String| ChangePasswordRequest {
            current_password: "different-current-password".into(),
            new_password,
        };
        assert!(validate_password_change(&request("中文密码".into())).is_ok());
        assert!(validate_password_change(&request("中文短".into())).is_err());
        assert!(validate_password_change(&request("x".repeat(1025))).is_err());
        assert!(validate_password_change(&request("different-current-password".into())).is_err());
    }
}
