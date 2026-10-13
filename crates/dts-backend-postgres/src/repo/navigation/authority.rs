use super::storage::error;
use dts_backend::{
    BackendError,
    repo::navigation::{NavigationScope, validation},
};
use sqlx::PgConnection;
use std::collections::BTreeSet;

pub(super) async fn permissions(
    conn: &mut PgConnection,
    scope: &NavigationScope,
    lock: bool,
) -> Result<BTreeSet<String>, BackendError> {
    validation::scope(scope)?;
    let query = if lock {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2
            AND t.status='active' AND u.status='active' FOR SHARE OF u,t"
    } else {
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2
            AND t.status='active' AND u.status='active'"
    };
    let actor: Option<String> = sqlx::query_scalar(query)
        .bind(&scope.tenant_id)
        .bind(&scope.actor_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
    if actor.is_none() {
        return Err(BackendError::Forbidden("导航用户或租户已失效".into()));
    }
    let query = if lock {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
            WHERE ur.user_id=$1 AND r.tenant_id=$2 FOR SHARE OF ur,r,rp"
    } else {
        "SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id
            WHERE ur.user_id=$1 AND r.tenant_id=$2"
    };
    let permissions: Vec<String> = sqlx::query_scalar(query)
        .bind(&scope.actor_id)
        .bind(&scope.tenant_id)
        .fetch_all(conn)
        .await
        .map_err(error)?;
    Ok(permissions.into_iter().collect())
}
pub(super) async fn manager(
    conn: &mut PgConnection,
    scope: &NavigationScope,
    lock: bool,
) -> Result<(), BackendError> {
    let permissions = permissions(conn, scope, lock).await?;
    if !permissions.contains("system:write") || !permissions.contains("user:manage") {
        return Err(BackendError::Forbidden(
            "分配、撤销及历史管理要求system:write和user:manage".into(),
        ));
    }
    Ok(())
}
