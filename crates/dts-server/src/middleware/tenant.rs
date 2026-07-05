use crate::models::auth::Claims;
use crate::models::error::AppError;

/// 验证请求者是否属于目标租户
/// super_admin 暂时不做跨租户特殊处理，后续 Phase 2 实现
pub fn verify_tenant_access(claims: &Claims, target_tenant_id: &str) -> Result<(), AppError> {
    if claims.tenant_id == target_tenant_id {
        return Ok(());
    }
    Err(AppError::Forbidden("跨租户访问被拒绝".into()))
}

/// 从 Claims 中提取租户 ID
pub fn tenant_id_from_claims(claims: &Claims) -> String {
    claims.tenant_id.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn make_claims(tenant_id: &str) -> Claims {
        Claims {
            sub: "user-1".into(),
            tenant_id: tenant_id.into(),
            username: "testuser".into(),
            exp: 1700000000,
            iat: 1699990000,
        }
    }

    // ========== verify_tenant_access Tests ==========

    #[test]
    fn verify_tenant_access_matching_tenant_id_passes() {
        let claims = make_claims("tenant-123");
        let result = verify_tenant_access(&claims, "tenant-123");
        assert!(result.is_ok());
    }

    #[test]
    fn verify_tenant_access_mismatched_tenant_id_returns_forbidden() {
        let claims = make_claims("tenant-123");
        let result = verify_tenant_access(&claims, "tenant-456");
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Forbidden(msg) => {
                assert!(msg.contains("跨租户"));
            },
            _ => panic!("Expected Forbidden"),
        }
    }

    #[test]
    fn verify_tenant_access_empty_tenant_id() {
        let claims = make_claims("");
        let result = verify_tenant_access(&claims, "");
        assert!(result.is_ok());
    }

    #[test]
    fn verify_tenant_access_different_case_tenant_ids() {
        let claims = make_claims("Tenant-A");
        let result = verify_tenant_access(&claims, "Tenant-a");
        assert!(result.is_err());
    }

    // ========== tenant_id_from_claims Tests ==========

    #[test]
    fn tenant_id_from_claims_returns_tenant_id() {
        let claims = make_claims("my-tenant");
        let tenant_id = tenant_id_from_claims(&claims);
        assert_eq!(tenant_id, "my-tenant");
    }

    #[test]
    fn tenant_id_from_claims_with_uuid_tenant_id() {
        let claims = make_claims("550e8400-e29b-41d4-a716-446655440000");
        let tenant_id = tenant_id_from_claims(&claims);
        assert_eq!(tenant_id, "550e8400-e29b-41d4-a716-446655440000");
    }
}
