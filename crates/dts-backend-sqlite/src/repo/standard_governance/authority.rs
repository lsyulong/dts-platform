use super::storage::*;
use dts_backend::BackendError;
use sqlx::SqliteConnection;

pub(super) async fn allowed(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    permissions: &[&str],
) -> Result<bool, BackendError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.tenant_id=?1 AND u.id=?2 AND u.status='active' AND t.status='active')").bind(tenant).bind(actor).fetch_one(&mut *conn).await.map_err(error)?;
    if !active {
        return Ok(false);
    }
    for permission in permissions {
        let actual:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2 AND rp.permission=?3)").bind(actor).bind(tenant).bind(permission).fetch_one(&mut *conn).await.map_err(error)?;
        if !actual {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) async fn require(
    conn: &mut SqliteConnection,
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
