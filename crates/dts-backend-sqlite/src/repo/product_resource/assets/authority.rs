use std::collections::BTreeSet;

use dts_backend::{BackendError, repo::assets::validation::invalid};
use sqlx::SqliteConnection;

use super::super::helpers::map_error;

pub(super) async fn permissions(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    _lock: bool,
) -> Result<BTreeSet<String>, BackendError> {
    let active: Option<String> = sqlx::query_scalar(
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id
        WHERE t.id=?1 AND u.id=?2 AND t.status='active' AND u.status='active'",
    )
    .bind(tenant)
    .bind(actor)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_error)?;
    if active.is_none() {
        return Err(invalid("当前资产操作用户或租户已失效"));
    }
    let values: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id
        JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2",
    )
    .bind(actor)
    .bind(tenant)
    .fetch_all(conn)
    .await
    .map_err(map_error)?;
    Ok(values.into_iter().collect())
}

pub(super) fn require(permissions: &BTreeSet<String>, value: &str) -> Result<(), BackendError> {
    if !permissions.contains(value) {
        return Err(invalid("当前用户缺少实际资产或来源权限"));
    }
    Ok(())
}
