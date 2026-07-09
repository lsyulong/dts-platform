use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,         // user_id
    pub tenant_id: String,
    pub username: String,
    pub exp: usize,          // expiry timestamp
    pub iat: usize,          // issued at timestamp
}

/// Create a JWT token
pub fn create_token(claims: &Claims, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate and decode a JWT token
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn generate_token(sub: &str, tenant_id: &str, secret: &str, hours_valid: i64) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: sub.into(),
            tenant_id: tenant_id.into(),
            username: "testuser".into(),
            exp: (now + chrono::Duration::hours(hours_valid)).timestamp() as usize,
            iat: now.timestamp() as usize,
        };
        create_token(&claims, secret).unwrap()
    }

    fn generate_expired_token(secret: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: "user-1".into(),
            tenant_id: "tenant-1".into(),
            username: "testuser".into(),
            exp: (now - chrono::Duration::hours(1)).timestamp() as usize,
            iat: (now - chrono::Duration::hours(2)).timestamp() as usize,
        };
        create_token(&claims, secret).unwrap()
    }

    #[test]
    fn validate_token_with_valid_token_returns_claims() {
        let secret = "test-secret";
        let token = generate_token("user-1", "tenant-1", secret, 1);
        let claims = validate_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.username, "testuser");
    }

    #[test]
    fn validate_token_with_expired_token_returns_err() {
        let secret = "test-secret";
        let token = generate_expired_token(secret);
        let result = validate_token(&token, secret);
        assert!(result.is_err());
    }

    #[test]
    fn validate_token_with_wrong_secret_returns_err() {
        let token = generate_token("user-1", "tenant-1", "correct-secret", 1);
        let result = validate_token(&token, "wrong-secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_token_with_malformed_token_returns_err() {
        let result = validate_token("not-a-valid-jwt", "secret");
        assert!(result.is_err());
    }

    #[test]
    fn validate_token_with_empty_token_returns_err() {
        let result = validate_token("", "secret");
        assert!(result.is_err());
    }

    #[test]
    fn create_token_and_validate_round_trip() {
        let secret = "round-trip-secret";
        let claims = Claims {
            sub: "user-round".into(),
            tenant_id: "tenant-round".into(),
            username: "roundtrip".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            iat: chrono::Utc::now().timestamp() as usize,
        };
        let token = create_token(&claims, secret).unwrap();
        let decoded = validate_token(&token, secret).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.tenant_id, claims.tenant_id);
        assert_eq!(decoded.username, claims.username);
    }
}
