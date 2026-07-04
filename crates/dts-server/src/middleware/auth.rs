use crate::models::auth::Claims;
use crate::models::error::AppError;
use actix_web::HttpRequest;
use jsonwebtoken::{decode, DecodingKey, Validation};

/// 从 HTTP 请求中提取并验证 JWT Claims
pub fn extract_claims(req: &HttpRequest, jwt_secret: &str) -> Result<Claims, AppError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("缺少 Authorization 请求头".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Authorization 格式错误，应为 Bearer <token>".into()))?;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| AppError::Unauthorized(format!("Token 无效或已过期: {}", e)))
}
