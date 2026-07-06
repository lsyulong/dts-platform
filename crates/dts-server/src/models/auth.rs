use serde::{Deserialize, Serialize};

/// 登录请求
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub tenant_slug: String,
}

/// 创建用户请求
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
    pub role_ids: Vec<String>,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub tenant_id: String,
    pub tenant_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 用户 ID
    pub sub: String,
    /// 租户 ID
    pub tenant_id: String,
    /// 用户名
    pub username: String,
    /// 过期时间 (unix timestamp seconds)
    pub exp: usize,
    /// 签发时间
    pub iat: usize,
}

/// 创建租户请求
#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
}

// ====== 从 dts-backend 重新导出，保持对外 API 兼容 ======
pub use dts_backend::types::TenantInfo;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== LoginRequest Tests ==========

    #[test]
    fn login_request_deserializes_valid_json() {
        let json = r#"{"username":"admin","password":"secret","tenant_slug":"default"}"#;
        let req: LoginRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.username, "admin");
        assert_eq!(req.password, "secret");
        assert_eq!(req.tenant_slug, "default");
    }

    #[test]
    fn login_request_missing_username_fails() {
        let json = r#"{"password":"secret","tenant_slug":"default"}"#;
        let result = serde_json::from_str::<LoginRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn login_request_missing_password_fails() {
        let json = r#"{"username":"admin","tenant_slug":"default"}"#;
        let result = serde_json::from_str::<LoginRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn login_request_missing_tenant_slug_fails() {
        let json = r#"{"username":"admin","password":"secret"}"#;
        let result = serde_json::from_str::<LoginRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn login_request_empty_json_fails() {
        let json = r#"{}"#;
        let result = serde_json::from_str::<LoginRequest>(json);
        assert!(result.is_err());
    }

    // ========== Claims Serialization Round-Trip Tests ==========

    #[test]
    fn claims_serialization_round_trip() {
        let claims = Claims {
            sub: "user-123".into(),
            tenant_id: "tenant-456".into(),
            username: "alice".into(),
            exp: 1700000000,
            iat: 1699990000,
        };
        let json = serde_json::to_string(&claims).unwrap();
        let parsed: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sub, claims.sub);
        assert_eq!(parsed.tenant_id, claims.tenant_id);
        assert_eq!(parsed.username, claims.username);
        assert_eq!(parsed.exp, claims.exp);
        assert_eq!(parsed.iat, claims.iat);
    }

    // ========== CreateUserRequest Tests ==========

    #[test]
    fn create_user_request_with_all_optional_fields() {
        let json = r#"{
            "username": "newuser",
            "email": "new@test.com",
            "password": "pass123",
            "display_name": "New User",
            "role_ids": ["role-1", "role-2"]
        }"#;
        let req: CreateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.username, "newuser");
        assert_eq!(req.email, "new@test.com");
        assert_eq!(req.password, "pass123");
        assert_eq!(req.display_name, Some("New User".into()));
        assert_eq!(req.role_ids, vec!["role-1", "role-2"]);
    }

    #[test]
    fn create_user_request_without_optional_fields() {
        let json = r#"{
            "username": "minuser",
            "email": "min@test.com",
            "password": "pass456",
            "role_ids": []
        }"#;
        let req: CreateUserRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.username, "minuser");
        assert_eq!(req.display_name, None);
        assert!(req.role_ids.is_empty());
    }

    #[test]
    fn create_user_request_missing_required_fields_fails() {
        let json = r#"{"username":"u","email":"e@t.com"}"#;
        let result = serde_json::from_str::<CreateUserRequest>(json);
        assert!(result.is_err());
    }

    // ========== TenantInfo Tests ==========

    #[test]
    fn tenant_info_fields_can_be_accessed() {
        let info = TenantInfo {
            id: "t1".into(),
            name: "Test Tenant".into(),
            slug: "test".into(),
            status: "active".into(),
            created_at: chrono::Utc::now(),
        };
        assert_eq!(info.id, "t1");
        assert_eq!(info.name, "Test Tenant");
        assert_eq!(info.slug, "test");
        assert_eq!(info.status, "active");
    }

    // ========== LoginResponse Tests ==========

    #[test]
    fn login_response_serializes_correctly() {
        let response = LoginResponse {
            token: "jwt-token-here".into(),
            user: UserInfo {
                id: "u1".into(),
                username: "admin".into(),
                email: "admin@test.com".into(),
                display_name: Some("Admin".into()),
                tenant_id: "t1".into(),
                tenant_name: "Default".into(),
                roles: vec!["admin".into()],
                permissions: vec!["task:read".into()],
            },
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["token"], "jwt-token-here");
        assert_eq!(json["user"]["username"], "admin");
        assert_eq!(json["user"]["tenant_name"], "Default");
    }

    // ========== UserInfo Tests ==========

    #[test]
    fn user_info_serialization_round_trip() {
        let user = UserInfo {
            id: "u1".into(),
            username: "admin".into(),
            email: "admin@test.com".into(),
            display_name: None,
            tenant_id: "t1".into(),
            tenant_name: "Default".into(),
            roles: vec!["tenant_admin".into()],
            permissions: vec!["task:read".into(), "task:write".into()],
        };
        let json = serde_json::to_string(&user).unwrap();
        let parsed: UserInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, user.id);
        assert_eq!(parsed.username, user.username);
        assert_eq!(parsed.display_name, user.display_name);
        assert_eq!(parsed.roles.len(), 1);
        assert_eq!(parsed.permissions.len(), 2);
    }

    // ========== CreateTenantRequest Tests ==========

    #[test]
    fn create_tenant_request_deserializes_correctly() {
        let json = r#"{"name":"My Tenant","slug":"my-tenant"}"#;
        let req: CreateTenantRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "My Tenant");
        assert_eq!(req.slug, "my-tenant");
    }
}
