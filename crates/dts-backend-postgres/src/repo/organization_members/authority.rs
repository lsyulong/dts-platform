use super::storage::error;
use dts_backend::{BackendError, repo::organization_members::*};
use sqlx::PgConnection;

pub(super) async fn check(
    conn: &mut PgConnection,
    s: &OrganizationScope,
    write: bool,
    lock: bool,
) -> Result<(), BackendError> {
    validation::scope(s)?;
    let query = if lock {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active' FOR SHARE OF u,t"
    } else {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active'"
    };
    let actual: Option<String> = sqlx::query_scalar(query)
        .bind(&s.tenant_id)
        .bind(&s.actor_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
    if actual.is_none() {
        return Err(BackendError::Forbidden("组织操作身份或租户已失效".into()));
    }
    let query = if lock {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2 FOR SHARE OF ur,r,rp"
    } else {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2"
    };
    let permissions: Vec<String> = sqlx::query_scalar(query)
        .bind(&s.actor_id)
        .bind(&s.tenant_id)
        .fetch_all(conn)
        .await
        .map_err(error)?;
    let permitted = if write {
        permissions.iter().any(|p| p == "system:write")
            && permissions.iter().any(|p| p == "user:manage")
    } else {
        permissions.iter().any(|p| p == "system:read")
    };
    if !permitted {
        return Err(BackendError::Forbidden("缺少组织成员查询或管理权限".into()));
    }
    Ok(())
}
