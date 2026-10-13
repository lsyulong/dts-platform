use dts_backend::{BackendError, repo::service_access::ServiceAuthority};
use sqlx::PgConnection;

use super::storage::{error, invalid};

pub(super) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    lock: bool,
) -> Result<ServiceAuthority, BackendError> {
    let sql = if lock {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active' FOR SHARE OF u,t"
    } else {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active'"
    };
    let user: Option<String> = sqlx::query_scalar(sql)
        .bind(tenant)
        .bind(actor)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
    if user.is_none() {
        return Ok(ServiceAuthority::default());
    }
    let sql = if lock {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
        WHERE ur.user_id=$1 AND r.tenant_id=$2 AND rp.permission='services:approve' FOR SHARE OF ur,r,rp"
    } else {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
        WHERE ur.user_id=$1 AND r.tenant_id=$2 AND rp.permission='services:approve'"
    };
    let rows: Vec<String> = sqlx::query_scalar(sql)
        .bind(actor)
        .bind(tenant)
        .fetch_all(conn)
        .await
        .map_err(error)?;
    Ok(ServiceAuthority {
        active: true,
        can_approve: rows.iter().any(|p| p == "services:approve"),
    })
}

pub(super) async fn active(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
) -> Result<(), BackendError> {
    let user: Option<String> = sqlx::query_scalar(
        "SELECT id FROM \"user\" WHERE tenant_id=$1 AND id=$2 AND status='active' FOR SHARE",
    )
    .bind(tenant)
    .bind(actor)
    .fetch_optional(conn)
    .await
    .map_err(error)?;
    if user.is_none() {
        return Err(invalid("应用所有者不存在、不属于租户或已停用"));
    }
    Ok(())
}

pub(super) fn manage(
    authority: &ServiceAuthority,
    actor: &str,
    owner: &str,
) -> Result<(), BackendError> {
    if !authority.active || (actor != owner && !authority.can_approve) {
        return Err(invalid("仅实际应用所有者或服务审批管理者可操作"));
    }
    Ok(())
}

pub(super) fn approve(
    authority: &ServiceAuthority,
    actor: &str,
    owner: &str,
    requester: &str,
) -> Result<(), BackendError> {
    if !authority.active || !authority.can_approve || actor == owner || actor == requester {
        return Err(invalid(
            "需要实际服务审批权限且不能审核本人申请或自己的应用",
        ));
    }
    Ok(())
}
