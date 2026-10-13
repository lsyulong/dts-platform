use super::storage::*;
use dts_backend::BackendError;
use sqlx::PgConnection;

pub(super) async fn allowed(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    permissions: &[&str],
) -> Result<bool, BackendError> {
    let active:Option<String>=sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.tenant_id=$1 AND u.id=$2 AND u.status='active' AND t.status='active' FOR SHARE OF u,t").bind(tenant).bind(actor).fetch_optional(&mut *conn).await.map_err(error)?;
    if active.is_none() {
        return Ok(false);
    }
    for permission in permissions {
        let actual:Option<String>=sqlx::query_scalar("SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2 AND rp.permission=$3 LIMIT 1 FOR SHARE OF ur,r,rp").bind(actor).bind(tenant).bind(permission).fetch_optional(&mut *conn).await.map_err(error)?;
        if actual.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) async fn require(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    permissions: &[&str],
) -> Result<(), BackendError> {
    if !allowed(conn, tenant, actor, permissions).await? {
        return Err(BackendError::Forbidden(
            "用户/租户已停用或缺少实际标准治理权限".into(),
        ));
    }
    Ok(())
}
