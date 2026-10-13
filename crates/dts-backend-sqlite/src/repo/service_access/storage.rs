use super::super::product_resource::{SqliteProductResourceRepo as Resources, helpers};
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, ResourceWrite};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(super) use helpers::map_error as error;
pub(super) fn invalid(value: &str) -> BackendError {
    BackendError::ConstraintViolation(value.into())
}
pub(super) fn conflict(value: &str) -> BackendError {
    BackendError::Conflict(value.into())
}
pub(super) fn parse_time(value: &str) -> Result<DateTime<Utc>, BackendError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| BackendError::Internal("服务身份时间格式无效".into()))
}
pub(super) fn time(row: &SqliteRow, field: &str) -> Result<DateTime<Utc>, BackendError> {
    parse_time(&row.try_get::<String, _>(field).map_err(error)?)
}
pub(super) fn optional_time(
    row: &SqliteRow,
    field: &str,
) -> Result<Option<DateTime<Utc>>, BackendError> {
    row.try_get::<Option<String>, _>(field)
        .map_err(error)?
        .as_deref()
        .map(parse_time)
        .transpose()
}
pub(super) async fn now(conn: &mut SqliteConnection) -> Result<DateTime<Utc>, BackendError> {
    let value: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')")
        .fetch_one(conn)
        .await
        .map_err(error)?;
    parse_time(&value)
}
pub(super) async fn resource(
    conn: &mut SqliteConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<Option<ProductResource>, BackendError> {
    Resources::read_resource(conn, tenant, kind, id).await
}
pub(super) async fn write(
    conn: &mut SqliteConnection,
    actor: &str,
    before: Option<&ProductResource>,
    resource: &ProductResource,
) -> Result<ProductResource, BackendError> {
    let fields = ResourceWrite {
        code: resource.code.clone(),
        name: resource.name.clone(),
        description: resource.description.clone(),
        parent_id: None,
        status: resource.status.clone(),
        properties: resource.properties.clone(),
        references: resource.references.clone(),
    };
    helpers::validate_graph(
        conn,
        &resource.tenant_id,
        &resource.kind,
        &resource.id,
        &fields,
    )
    .await?;
    if resource.properties.to_string().len() > 256 * 1024 {
        return Err(invalid("服务定义超过256KiB"));
    }
    if let Some(before) = before {
        let changed=sqlx::query("UPDATE product_resource SET code=?1,name=?2,description=?3,status=?4,properties=?5,version=?6,updated_at=?7 WHERE tenant_id=?8 AND kind=?9 AND id=?10 AND version=?11")
            .bind(&resource.code).bind(&resource.name).bind(&resource.description).bind(&resource.status).bind(resource.properties.to_string())
            .bind(resource.version).bind(resource.updated_at.to_rfc3339()).bind(&resource.tenant_id).bind(&resource.kind).bind(&resource.id).bind(before.version)
            .execute(&mut *conn).await.map_err(error)?.rows_affected();
        if changed != 1 {
            return Err(conflict("服务定义版本已改变"));
        }
    } else {
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,description,status,properties,version,created_by,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)")
            .bind(&resource.id).bind(&resource.tenant_id).bind(&resource.kind).bind(&resource.code).bind(&resource.name).bind(&resource.description)
            .bind(&resource.status).bind(resource.properties.to_string()).bind(resource.version).bind(&resource.created_by).bind(resource.updated_at.to_rfc3339())
            .execute(&mut *conn).await.map_err(error)?;
    }
    Resources::write_references(
        conn,
        &resource.tenant_id,
        &resource.kind,
        &resource.id,
        &resource.references,
    )
    .await?;
    helpers::audit(
        conn,
        actor,
        if before.is_some() { "update" } else { "create" },
        before,
        Some(resource),
    )
    .await?;
    Ok(resource.clone())
}
pub(super) fn bump(resource: &mut ProductResource, now: DateTime<Utc>) -> Result<(), BackendError> {
    resource.version = resource
        .version
        .checked_add(1)
        .ok_or_else(|| conflict("服务定义版本已满"))?;
    resource.updated_at = now;
    Ok(())
}
