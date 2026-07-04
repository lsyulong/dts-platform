use actix_web::{web, HttpRequest, HttpResponse};
use log::info;
use sqlx::Row;

use crate::middleware::auth;
use crate::models::auth::*;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;

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
) -> Result<HttpResponse, AppError> {
    let jwt_secret = &auth_svc.config.jwt_secret.clone();
    let claims = auth::extract_claims(&req, jwt_secret)?;

    let user_info = get_user_info(&auth_svc.pool, &claims.sub).await?;
    Ok(HttpResponse::Ok().json(user_info))
}

/// GET /api/v1/auth/permissions
pub async fn my_permissions(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = &auth_svc.config.jwt_secret.clone();
    let claims = auth::extract_claims(&req, jwt_secret)?;

    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT rp.permission FROM role_permission rp
         INNER JOIN user_role ur ON rp.role_id = ur.role_id
         WHERE ur.user_id = ?"
    )
    .bind(&claims.sub)
    .fetch_all(&auth_svc.pool)
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "permissions": permissions,
    })))
}

/// 获取用户详细信息
async fn get_user_info(pool: &sqlx::SqlitePool, user_id: &str) -> Result<UserInfo, AppError> {
    let row = sqlx::query(
        "SELECT u.id, u.username, u.email, u.display_name, u.tenant_id,
                t.name as tenant_name
         FROM user u
         JOIN tenant t ON u.tenant_id = t.id
         WHERE u.id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("用户不存在".into()))?;

    let uid: String = row.get(0);
    let username: String = row.get(1);
    let email: String = row.get(2);
    let display_name: Option<String> = row.get(3);
    let tenant_id: String = row.get(4);
    let tenant_name: String = row.get(5);

    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name FROM role r
         INNER JOIN user_role ur ON r.id = ur.role_id
         WHERE ur.user_id = ?"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT rp.permission FROM role_permission rp
         INNER JOIN user_role ur ON rp.role_id = ur.role_id
         WHERE ur.user_id = ?"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(UserInfo {
        id: uid,
        username,
        email,
        display_name,
        tenant_id,
        tenant_name,
        roles,
        permissions,
    })
}
