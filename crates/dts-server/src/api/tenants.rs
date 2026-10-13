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

use actix_web::{HttpRequest, HttpResponse, web};
use tracing::info;

use crate::middleware::auth;
use crate::models::auth::CreateTenantRequest;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;
use crate::services::tenant_svc::TenantService;

/// POST /api/v1/tenants
pub async fn create(
    req: HttpRequest,
    tenant_svc: web::Data<TenantService>,
    body: web::Json<CreateTenantRequest>,
) -> Result<HttpResponse, AppError> {
    auth::require_permission(&req, "tenant:create").await?;
    let password = body
        .admin_password
        .as_deref()
        .filter(|password| password.len() >= 12)
        .ok_or_else(|| AppError::BadRequest("请提供至少 12 字节的管理员密码".into()))?;
    if body.name.trim().is_empty()
        || body.slug.is_empty()
        || !body
            .slug
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(AppError::BadRequest(
            "租户名称不能为空，标识仅允许字母、数字、横线和下划线".into(),
        ));
    }
    let password_hash =
        AuthService::hash_password_sync(password).map_err(|e| AppError::Internal(e.to_string()))?;

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
        "message": "租户创建成功"
    })))
}

/// GET /api/v1/tenants
pub async fn list(
    req: HttpRequest,
    tenant_svc: web::Data<TenantService>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "user:manage").await?;
    let tenant = tenant_svc
        .get_by_id(&claims.tenant_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": [tenant]
    })))
}
