use actix_web::{HttpRequest, HttpResponse, web};
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
         WHERE ur.user_id = $1",
    )
    .bind(&claims.sub)
    .fetch_all(&auth_svc.pool)
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "permissions": permissions,
    })))
}

/// 获取用户详细信息
async fn get_user_info(pool: &sqlx::PgPool, user_id: &str) -> Result<UserInfo, AppError> {
    let row = sqlx::query(
        "SELECT u.id, u.username, u.email, u.display_name, u.tenant_id,
                t.name as tenant_name
         FROM \"user\" u
         JOIN tenant t ON u.tenant_id = t.id
         WHERE u.id = $1",
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
         WHERE ur.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT rp.permission FROM role_permission rp
         INNER JOIN user_role ur ON rp.role_id = ur.role_id
         WHERE ur.user_id = $1",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use pretty_assertions::assert_eq;
    use sqlx::PgPool;

    async fn create_test_pool() -> PgPool {
        use sqlx::postgres::PgPoolOptions;
        PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://localhost:5432/dts_test")
            .await
            .expect("Failed to create test pool")
    }

    fn test_config() -> AppConfig {
        AppConfig::for_test()
    }

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

    // ========== AuthService Construction Tests ==========

    #[tokio::test]
    async fn auth_service_new_creates_service() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);
        assert_eq!(svc.config.jwt_expiry_hours, 24);
    }

    #[tokio::test]
    async fn auth_service_clone_works() {
        let pool = create_test_pool().await;
        let config = test_config();
        let svc = AuthService::new(pool, config);
        let cloned = svc.clone();
        assert_eq!(cloned.config.jwt_secret, svc.config.jwt_secret);
    }
}
