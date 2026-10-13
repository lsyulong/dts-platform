use super::storage;
use dts_backend::{BackendError, repo::organization_members::validation};
use sqlx::SqliteConnection;

pub(super) async fn current(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
) -> Result<Option<String>, BackendError> {
    validation::text(tenant, 128)?;
    validation::text(actor, 128)?;
    let identity:Option<String>=sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=?1 AND u.id=?2 AND t.status='active' AND u.status='active'")
        .bind(tenant).bind(actor).fetch_optional(&mut *conn).await.map_err(storage::error)?;
    if identity.is_none() {
        return Err(BackendError::Forbidden("组织读取身份或租户已失效".into()));
    }
    let Some(id) = storage::membership(conn, tenant, actor)
        .await?
        .and_then(|m| m.organization_id)
    else {
        return Ok(None);
    };
    let organization = storage::organization(conn, tenant, &id).await?;
    match storage::available(conn, tenant, &organization).await {
        Ok(()) => Ok(Some(id)),
        Err(BackendError::Conflict(_)) => Ok(None),
        Err(error) => Err(error),
    }
}
