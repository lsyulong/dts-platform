use super::storage::error;
use dts_backend::{BackendError, repo::data_access::DataAccessAuthority};
use sqlx::{PgConnection, Row};

pub(crate) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    lock: bool,
) -> Result<DataAccessAuthority, BackendError> {
    let active: Option<String> = sqlx::query_scalar(
        "SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id
        WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active'",
    )
    .bind(tenant)
    .bind(actor)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    if active.is_none() {
        return Ok(DataAccessAuthority::default());
    }
    let base =
        "SELECT r.name,r.is_system,rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id
        JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2
        AND rp.permission IN ('data_security:approve','data_security:all_data')";
    let statement = if lock {
        format!("{base} FOR SHARE OF ur,r,rp")
    } else {
        base.into()
    };
    let rows = sqlx::query(&statement)
        .bind(actor)
        .bind(tenant)
        .fetch_all(conn)
        .await
        .map_err(error)?;
    let mut authority = DataAccessAuthority {
        active: true,
        ..Default::default()
    };
    for row in rows {
        let permission: String = row.try_get("permission").map_err(error)?;
        if permission == "data_security:approve" {
            authority.can_approve = true;
        }
        if permission == "data_security:all_data"
            && row.try_get::<String, _>("name").map_err(error)? == "tenant_admin"
            && row.try_get::<bool, _>("is_system").map_err(error)?
        {
            authority.all_data = true;
        }
    }
    Ok(authority)
}
