use actix_web::{HttpResponse, web};
use log::info;

use crate::models::auth::CreateTenantRequest;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;
use crate::services::tenant_svc::TenantService;

/// POST /api/v1/tenants
pub async fn create(
    tenant_svc: web::Data<TenantService>,
    _auth_svc: web::Data<AuthService>,
    body: web::Json<CreateTenantRequest>,
) -> Result<HttpResponse, AppError> {
    let password_hash = AuthService::hash_password_sync("admin123")
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let admin_username = format!("admin_{}", body.slug);
    let admin_email = format!("admin@{}", body.slug);

    let tenant_id = tenant_svc
        .create_tenant(
            &body.name,
            &body.slug,
            &admin_username,
            &admin_email,
            &password_hash,
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    info!(
        "新租户创建: {} ({}), 管理员: {}",
        body.name, body.slug, admin_username
    );

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": tenant_id,
        "name": body.name,
        "slug": body.slug,
        "admin_username": admin_username,
        "message": "租户创建成功，默认管理员密码为 admin123，请及时修改"
    })))
}

/// GET /api/v1/tenants
pub async fn list(tenant_svc: web::Data<TenantService>) -> Result<HttpResponse, AppError> {
    let tenants = tenant_svc
        .list()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": tenants
    })))
}
