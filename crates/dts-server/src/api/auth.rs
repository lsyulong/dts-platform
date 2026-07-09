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
    auth_svc: web::Data<AuthService>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    info!("用户登录: {} @ {}", body.username, body.tenant_slug);
    let response = auth_svc
        .login(&body)
        .await
        .map_err(|e| AppError::Unauthorized(e.to_string()))?;
    Ok(HttpResponse::Ok().json(response))
}

/// GET /api/v1/auth/me
pub async fn me(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_repo: web::Data<Arc<dyn UserRepo>>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;

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
    auth_svc: web::Data<AuthService>,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;

    let permissions = role_repo.find_user_permissions(&claims.sub).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "permissions": permissions,
    })))
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
