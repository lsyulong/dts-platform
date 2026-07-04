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
