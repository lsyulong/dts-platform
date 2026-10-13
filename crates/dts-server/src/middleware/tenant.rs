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

use crate::models::auth::AuthIdentity;
use crate::models::error::AppError;

/// 验证请求者是否属于目标租户
pub fn verify_tenant_access(claims: &AuthIdentity, target_tenant_id: &str) -> Result<(), AppError> {
    if !claims.tenant_id.is_empty() && claims.tenant_id == target_tenant_id {
        return Ok(());
    }
    Err(AppError::Forbidden("跨租户访问被拒绝".into()))
}

/// 从已认证身份中提取租户 ID
pub fn tenant_id_from_identity(claims: &AuthIdentity) -> String {
    claims.tenant_id.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn make_identity(tenant_id: &str) -> AuthIdentity {
        AuthIdentity {
            sub: "user-1".into(),
            tenant_id: tenant_id.into(),
            username: "testuser".into(),
        }
    }

    // ========== verify_tenant_access Tests ==========

    #[test]
    fn verify_tenant_access_matching_tenant_id_passes() {
        let claims = make_identity("tenant-123");
        let result = verify_tenant_access(&claims, "tenant-123");
        assert!(result.is_ok());
    }

    #[test]
    fn verify_tenant_access_mismatched_tenant_id_returns_forbidden() {
        let claims = make_identity("tenant-123");
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
        let claims = make_identity("");
        let result = verify_tenant_access(&claims, "");
        assert!(result.is_err());
    }

    #[test]
    fn verify_tenant_access_different_case_tenant_ids() {
        let claims = make_identity("Tenant-A");
        let result = verify_tenant_access(&claims, "Tenant-a");
        assert!(result.is_err());
    }

    // ========== tenant_id_from_identity Tests ==========

    #[test]
    fn tenant_id_from_identity_returns_tenant_id() {
        let claims = make_identity("my-tenant");
        let tenant_id = tenant_id_from_identity(&claims);
        assert_eq!(tenant_id, "my-tenant");
    }

    #[test]
    fn tenant_id_from_identity_with_uuid_tenant_id() {
        let claims = make_identity("550e8400-e29b-41d4-a716-446655440000");
        let tenant_id = tenant_id_from_identity(&claims);
        assert_eq!(tenant_id, "550e8400-e29b-41d4-a716-446655440000");
    }
}
