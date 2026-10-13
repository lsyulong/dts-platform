use super::storage::error;
use dts_backend::{BackendError, repo::service_access::ServiceAuthority};
use sqlx::SqliteConnection;

pub(super) async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
) -> Result<ServiceAuthority, BackendError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=?1 AND u.id=?2 AND t.status='active' AND u.status='active')")
        .bind(tenant).bind(actor).fetch_one(&mut *conn).await.map_err(error)?;
    if !active {
        return Ok(ServiceAuthority::default());
    }
    let can_approve:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2 AND rp.permission='services:approve')")
        .bind(actor).bind(tenant).fetch_one(conn).await.map_err(error)?;
    Ok(ServiceAuthority {
        active,
        can_approve,
    })
}
pub(super) async fn active(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
) -> Result<ServiceAuthority, BackendError> {
    let value = get(conn, tenant, actor).await?;
    if !value.active {
        return Err(BackendError::Forbidden(
            "服务操作租户或真实用户已停用".into(),
        ));
    }
    Ok(value)
}
pub(super) async fn manage(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    owner: &str,
) -> Result<ServiceAuthority, BackendError> {
    let value = active(conn, tenant, actor).await?;
    if actor != owner && !value.can_approve {
        return Err(BackendError::Forbidden(
            "服务应用仅本人或明确审批管理员可管理".into(),
        ));
    }
    Ok(value)
}
