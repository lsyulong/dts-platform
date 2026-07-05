use crate::config::AppConfig;
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

/// 从 AppConfig 获取 JWT Secret（用于 API handler 中统一获取 secret）
/// 优先使用传入的 config，如果不可用则回退到环境变量
pub fn get_jwt_secret(config: Option<&AppConfig>) -> String {
    config
        .map(|c| c.jwt_secret.clone())
        .unwrap_or_else(|| {
            std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::auth::Claims;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use pretty_assertions::assert_eq;

    const TEST_SECRET: &str = "test-secret-for-unit-tests";

    /// Helper: generate a valid JWT for testing
    fn generate_valid_token(secret: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: "user-1".into(),
            tenant_id: "tenant-1".into(),
            username: "testuser".into(),
            exp: (now + chrono::Duration::hours(1)).timestamp() as usize,
            iat: now.timestamp() as usize,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    /// Helper: generate an expired JWT
    fn generate_expired_token(secret: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: "user-1".into(),
            tenant_id: "tenant-1".into(),
            username: "testuser".into(),
            exp: (now - chrono::Duration::hours(1)).timestamp() as usize,
            iat: (now - chrono::Duration::hours(2)).timestamp() as usize,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    // ========== extract_claims Tests ==========

    #[test]
    fn extract_claims_with_valid_token_returns_claims() {
        let token = generate_valid_token(TEST_SECRET);
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_ok());
        let claims = result.unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.username, "testuser");
    }

    #[test]
    fn extract_claims_missing_authorization_header_returns_unauthorized() {
        let req = actix_web::test::TestRequest::default().to_http_request();
        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Authorization"));
            }
            _ => panic!("Expected Unauthorized"),
        }
    }

    #[test]
    fn extract_claims_wrong_prefix_returns_unauthorized() {
        let token = generate_valid_token(TEST_SECRET);
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", format!("Basic {}", token)))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Bearer"));
            }
            _ => panic!("Expected Unauthorized"),
        }
    }

    #[test]
    fn extract_claims_no_prefix_returns_unauthorized() {
        let token = generate_valid_token(TEST_SECRET);
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", token.clone()))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn extract_claims_invalid_jwt_returns_unauthorized() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", "Bearer not.a.valid.token"))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Token"));
            }
            _ => panic!("Expected Unauthorized"),
        }
    }

    #[test]
    fn extract_claims_expired_token_returns_unauthorized() {
        let token = generate_expired_token(TEST_SECRET);
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Token") || msg.contains("expired"));
            }
            _ => panic!("Expected Unauthorized"),
        }
    }

    #[test]
    fn extract_claims_wrong_secret_returns_unauthorized() {
        let token = generate_valid_token("correct-secret");
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_http_request();

        let result = extract_claims(&req, "wrong-secret");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Unauthorized(_) => {}
            _ => panic!("Expected Unauthorized"),
        }
    }

    #[test]
    fn extract_claims_empty_bearer_token_returns_unauthorized() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", "Bearer "))
            .to_http_request();

        let result = extract_claims(&req, TEST_SECRET);
        assert!(result.is_err());
    }

    // ========== get_jwt_secret Tests ==========

    #[test]
    fn get_jwt_secret_with_some_config_returns_config_secret() {
        let config = AppConfig::for_test();
        let expected = config.jwt_secret.clone();
        let secret = get_jwt_secret(Some(&config));
        assert_eq!(secret, expected);
    }

    #[test]
    fn get_jwt_secret_with_none_falls_back_to_env_or_default() {
        // Clean up env var first
        std::env::remove_var("JWT_SECRET");
        let secret = get_jwt_secret(None);
        // Should fall back to default or env var
        assert!(!secret.is_empty());
    }

    #[test]
    fn get_jwt_secret_with_none_and_env_var_uses_env() {
        std::env::set_var("JWT_SECRET", "env-secret-value");
        let secret = get_jwt_secret(None);
        assert_eq!(secret, "env-secret-value");
        std::env::remove_var("JWT_SECRET");
    }
}
