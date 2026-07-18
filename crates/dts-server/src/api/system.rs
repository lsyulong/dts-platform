use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use tracing::info;

use crate::middleware::auth;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;
use crate::services::user_svc::UserService;
use dts_backend::repo::{RoleRepo, TenantRepo};
use dts_backend::types::*;
use uuid::Uuid;

pub const ALL_PERMISSIONS: &[&str] = &[
    "task:read",
    "task:write",
    "task:delete",
    "ds:read",
    "ds:write",
    "ds:delete",
    "alert:read",
    "alert:write",
    "monitor:read",
    "ops:read",
    "admin:*",
    "system:read",
    "system:write",
];

// ── Request types ────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct UsersQuery {
    pub tenant_id: Option<String>,
    pub search: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateUserReq {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateUserReq {
    pub display_name: Option<String>,
    #[serde(default)]
    pub role_ids: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ResetPasswordReq {
    pub new_password: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateRoleReq {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateRoleReq {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateTenantReq {
    pub name: Option<String>,
    pub status: Option<String>,
}

// ── Users ─────────────────────────────────────────────────────────────

/// GET /api/v1/system/users
pub async fn list_users(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_svc: web::Data<UserService>,
    query: web::Query<UsersQuery>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let users = user_svc
        .list_users(
            query.tenant_id.as_deref(),
            query.search.as_deref(),
            query.status.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": users })))
}

/// POST /api/v1/system/users
pub async fn create_user(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_svc: web::Data<UserService>,
    body: web::Json<CreateUserReq>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;

    let user = user_svc
        .create_user(
            &claims.tenant_id,
            &body.username,
            &body.email,
            &body.password,
            body.display_name.as_deref(),
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    for role_id in &body.role_ids {
        user_svc
            .user_repo
            .assign_role(&user.id, role_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    info!("用户 '{}' 创建成功", body.username);
    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": user.id,
        "username": user.username,
        "message": "用户创建成功"
    })))
}

/// PUT /api/v1/system/users/{id}
pub async fn update_user(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_svc: web::Data<UserService>,
    path: web::Path<String>,
    body: web::Json<UpdateUserReq>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let user_id = path.into_inner();

    user_svc
        .update_user(&user_id, body.display_name.clone(), body.status.clone())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if let Some(ref role_ids) = body.role_ids {
        user_svc
            .update_user_roles(&user_id, role_ids)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    info!("用户 {} 更新成功", user_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "用户更新成功" })))
}

/// POST /api/v1/system/users/{id}/reset-password
pub async fn reset_password(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_svc: web::Data<UserService>,
    path: web::Path<String>,
    body: web::Json<ResetPasswordReq>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let user_id = path.into_inner();

    user_svc
        .reset_user_password(&user_id, &body.new_password)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    info!("用户 {} 密码已重置", user_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "密码重置成功" })))
}

/// DELETE /api/v1/system/users/{id}
pub async fn delete_user(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    user_svc: web::Data<UserService>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let user_id = path.into_inner();

    user_svc
        .soft_delete_user(&user_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    info!("用户 {} 已软删除", user_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "用户已删除" })))
}

// ── Roles ─────────────────────────────────────────────────────────────

/// GET /api/v1/system/roles
pub async fn list_roles(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let roles = role_repo
        .list_all(None)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": roles })))
}

/// POST /api/v1/system/roles
pub async fn create_role(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
    body: web::Json<CreateRoleReq>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;

    let role_id = role_repo
        .create_with_permissions(&CreateRoleParams {
            id: Uuid::new_v4().to_string(),
            tenant_id: claims.tenant_id,
            name: body.name.clone(),
            description: body.description.clone().unwrap_or_default(),
            is_system: false,
            permissions: body.permissions.clone(),
        })
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    info!("角色 '{}' 创建成功", body.name);
    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": role_id,
        "message": "角色创建成功"
    })))
}

/// PUT /api/v1/system/roles/{id}
pub async fn update_role(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
    path: web::Path<String>,
    body: web::Json<UpdateRoleReq>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let role_id = path.into_inner();

    role_repo
        .update(
            &role_id,
            &UpdateRoleParams {
                name: body.name.clone(),
                permissions: body.permissions.clone(),
            },
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    info!("角色 {} 更新成功", role_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "角色更新成功" })))
}

/// DELETE /api/v1/system/roles/{id}
pub async fn delete_role(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    role_repo: web::Data<Arc<dyn RoleRepo>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let role_id = path.into_inner();

    role_repo
        .delete(&role_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    info!("角色 {} 已删除", role_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "角色已删除" })))
}

// ── Tenants ───────────────────────────────────────────────────────────

/// GET /api/v1/system/tenants
pub async fn list_tenants(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    tenant_repo: web::Data<Arc<dyn TenantRepo>>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let tenants = tenant_repo
        .list_all()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": tenants })))
}

/// PUT /api/v1/system/tenants/{id}
pub async fn update_tenant(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
    tenant_repo: web::Data<Arc<dyn TenantRepo>>,
    path: web::Path<String>,
    body: web::Json<UpdateTenantReq>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let tenant_id = path.into_inner();

    tenant_repo
        .update(
            &tenant_id,
            &UpdateTenantParams {
                name: body.name.clone(),
                status: body.status.clone(),
            },
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    info!("租户 {} 更新成功", tenant_id);
    Ok(HttpResponse::Ok().json(serde_json::json!({ "message": "租户更新成功" })))
}

// ── Permissions ───────────────────────────────────────────────────────

/// GET /api/v1/system/permissions
pub async fn list_permissions(
    req: HttpRequest,
    auth_svc: web::Data<AuthService>,
) -> Result<HttpResponse, AppError> {
    let _claims = auth::extract_claims(&req, &auth_svc.jwt_secret)?;
    let perms: Vec<serde_json::Value> = ALL_PERMISSIONS
        .iter()
        .map(|&p| {
            serde_json::json!({
                "code": p,
                "group": p.split(':').next().unwrap_or("other"),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": perms })))
}
