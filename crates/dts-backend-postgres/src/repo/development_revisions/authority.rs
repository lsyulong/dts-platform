use super::storage::error;
use dts_backend::{BackendError, repo::development_revisions::*};
use sqlx::PgConnection;
pub(super) async fn check(
    conn: &mut PgConnection,
    s: &DevelopmentRevisionScope,
    write: bool,
    lock: bool,
) -> Result<(), BackendError> {
    validation::scope(s)?;
    let actual:Option<String>=sqlx::query_scalar(if lock { "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active' FOR SHARE OF u,t" } else { "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active'" })
        .bind(&s.tenant_id).bind(&s.actor_id).fetch_optional(&mut *conn).await.map_err(error)?;
    if actual.is_none() {
        return Err(BackendError::Forbidden("修订操作身份或租户已失效".into()));
    }
    let permissions:Vec<String>=sqlx::query_scalar(if lock { "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2 FOR SHARE OF ur,r,rp" } else { "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2" })
        .bind(&s.actor_id).bind(&s.tenant_id).fetch_all(conn).await.map_err(error)?;
    if !permissions.iter().any(|p| p == "development:read")
        || (write && !permissions.iter().any(|p| p == "development:write"))
    {
        return Err(BackendError::Forbidden(
            "缺少真实开发定义读取或修订权限".into(),
        ));
    }
    Ok(())
}
