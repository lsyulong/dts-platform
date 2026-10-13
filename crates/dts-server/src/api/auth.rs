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

use actix_web::{HttpRequest, HttpResponse, web};
use tracing::info;

use crate::middleware::auth;
use crate::models::auth::*;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;
use dts_backend::repo::{RoleRepo, UserRepo};

/// POST /api/v1/auth/login
pub async fn login(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    if body.username.is_empty()
        || body.username.len() > 256
        || body.tenant_slug.len() > 128
        || body.password.len() > 4096
    {
        return Err(AppError::BadRequest("登录字段长度无效".into()));
    }
    info!("用户登录: {} @ {}", body.username, body.tenant_slug);
    let result = auth_svc.login(&body).await;
    if let Some(events) = req.app_data::<web::Data<crate::services::product::ProductService>>() {
        let tenant = match &result {
            Ok(response) => Some(response.user.tenant_id.clone()),
            Err(_) => auth_svc
                .tenant_repo
                .find_by_slug(&body.tenant_slug)
                .await?
                .map(|tenant| tenant.id),
        };
        if let Some(tenant_id) = tenant {
            let user_id = result
                .as_ref()
                .ok()
                .map(|response| response.user.id.as_str());
            let source_ip = req
                .peer_addr()
                .map(|addr| addr.ip().to_string())
                .unwrap_or_default();
            if let Err(error) = events
                .record_login(
                    &tenant_id,
                    user_id,
                    &body.username,
                    &source_ip,
                    result.is_ok(),
                )
                .await
            {
                tracing::error!("登录审计持久化失败");
                if let Ok(response) = &result {
                    let _ = auth_svc.logout(&response.token).await;
                    return Err(error);
                }
            }
        }
    }
    Ok(HttpResponse::Ok().json(result?))
}

/// POST /api/v1/auth/logout
pub async fn logout(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
) -> Result<HttpResponse, AppError> {
    auth_svc.logout(auth::bearer_token(&req)?).await?;
    Ok(HttpResponse::NoContent().finish())
}

/// GET /api/v1/auth/me
pub async fn me(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepo>>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::authenticate_request(&req).await?;

    let user_detail = user_repo
        .find_detail_by_id(&claims.sub)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;

    let user_info = UserInfo {
        id: user_detail.id,
        username: user_detail.username,
        email: user_detail.email,
        display_name: user_detail.display_name,
        tenant_id: user_detail.tenant_id,
        tenant_name: user_detail.tenant_name,
        roles: user_detail.roles,
        permissions: user_detail.permissions,
    };

    Ok(HttpResponse::Ok().json(user_info))
}

/// GET /api/v1/auth/permissions
pub async fn my_permissions(
    req: HttpRequest,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::authenticate_request(&req).await?;

    let permissions = role_repo.find_user_permissions(&claims.sub).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "permissions": permissions,
    })))
}

pub async fn update_profile(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    body: web::Json<UpdateProfileRequest>,
) -> Result<HttpResponse, AppError> {
    let identity = auth::authenticate_request(&req).await?;
    let user = auth_svc.update_self_profile(&identity, &body).await?;
    Ok(HttpResponse::Ok().json(user))
}

pub async fn change_password(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    body: web::Json<ChangePasswordRequest>,
) -> Result<HttpResponse, AppError> {
    let identity = auth::authenticate_request(&req).await?;
    auth_svc.change_self_password(&identity, &body).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== LoginRequest Deserialization Tests ==========

    #[test]
    fn login_request_valid_json() {
        let json = serde_json::json!({
            "username": "admin",
            "password": "secret123",
            "tenant_slug": "default"
        });
        let req: LoginRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.username, "admin");
        assert_eq!(req.password, "secret123");
        assert_eq!(req.tenant_slug, "default");
    }

    #[test]
    fn login_request_missing_username_fails() {
        let json = serde_json::json!({
            "password": "secret",
            "tenant_slug": "default"
        });
        let result = serde_json::from_value::<LoginRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn login_request_missing_password_fails() {
        let json = serde_json::json!({
            "username": "admin",
            "tenant_slug": "default"
        });
        let result = serde_json::from_value::<LoginRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn login_request_missing_tenant_slug_fails() {
        let json = serde_json::json!({
            "username": "admin",
            "password": "secret"
        });
        let result = serde_json::from_value::<LoginRequest>(json);
        assert!(result.is_err());
    }
}
