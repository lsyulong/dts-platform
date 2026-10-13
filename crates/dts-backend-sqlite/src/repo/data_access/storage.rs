use super::super::product_resource::{SqliteProductResourceRepo as Resources, helpers};
use chrono::{DateTime, Utc};
use dts_backend::{
    BackendError, ProductResource, ResourceWrite,
    repo::{data_access::DataAccessTarget, metadata_catalog::*},
};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(super) use helpers::map_error as error;
pub(super) const KIND: &str = "data_access_request";
pub(super) fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) fn parse_time(value: &str) -> Result<DateTime<Utc>, BackendError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| BackendError::Internal("授权持久化时间无效".into()))
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
    id: &str,
) -> Result<Option<ProductResource>, BackendError> {
    Resources::read_resource(conn, tenant, KIND, id).await
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
    helpers::validate_graph(&mut *conn, &resource.tenant_id, KIND, &resource.id, &fields).await?;
    let properties = resource.properties.to_string();
    if properties.len() > 256 * 1024 {
        return Err(invalid("申请属性超过256KiB"));
    }
    if let Some(previous) = before {
        let changed=sqlx::query("UPDATE product_resource SET code=?1,name=?2,description=?3,status=?4,properties=?5,version=?6,updated_at=?7
            WHERE tenant_id=?8 AND kind='data_access_request' AND id=?9 AND version=?10")
            .bind(&resource.code).bind(&resource.name).bind(&resource.description).bind(&resource.status).bind(properties).bind(resource.version)
            .bind(resource.updated_at.to_rfc3339()).bind(&resource.tenant_id).bind(&resource.id).bind(previous.version)
            .execute(&mut *conn).await.map_err(error)?.rows_affected();
        if changed != 1 {
            return Err(conflict("申请版本已改变"));
        }
    } else {
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,description,status,properties,version,created_by,created_at,updated_at)
            VALUES(?1,?2,'data_access_request',?3,?4,?5,?6,?7,?8,?9,?10,?10)")
            .bind(&resource.id).bind(&resource.tenant_id).bind(&resource.code).bind(&resource.name).bind(&resource.description).bind(&resource.status)
            .bind(properties).bind(resource.version).bind(&resource.created_by).bind(resource.updated_at.to_rfc3339())
            .execute(&mut *conn).await.map_err(error)?;
    }
    Resources::write_references(
        &mut *conn,
        &resource.tenant_id,
        KIND,
        &resource.id,
        &resource.references,
    )
    .await?;
    let saved = resource.clone();
    helpers::audit(
        conn,
        actor,
        if before.is_some() { "update" } else { "create" },
        before,
        Some(&saved),
    )
    .await?;
    Ok(saved)
}

pub(crate) async fn verified(
    conn: &mut SqliteConnection,
    tenant: &str,
    target: &DataAccessTarget,
) -> Result<bool, BackendError> {
    let row=sqlx::query("SELECT c.engine,c.object_version,c.table_catalog,c.schema_name,c.table_name,c.table_type,
        c.columns_snapshot AS encoded_columns,c.source_fingerprint,r.properties,r.version FROM metadata_catalog c JOIN product_resource r
        ON r.tenant_id=c.tenant_id AND r.kind=c.object_kind AND r.id=c.object_id WHERE c.tenant_id=?1 AND c.object_id=?2
        AND c.discovery_status='present' AND c.engine='swanlake' AND r.status='published'")
        .bind(tenant).bind(&target.metadata_id).fetch_optional(&mut *conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(false) };
    let table = MetadataTable {
        table_catalog: row.try_get("table_catalog").map_err(error)?,
        schema_name: row.try_get("schema_name").map_err(error)?,
        table_name: row.try_get("table_name").map_err(error)?,
        table_type: row.try_get("table_type").map_err(error)?,
        columns: serde_json::from_str(&row.try_get::<String, _>("encoded_columns").map_err(error)?)
            .map_err(|_| invalid("真实字段快照无效"))?,
    };
    let binding = MetadataEngineBinding {
        engine: row.try_get("engine").map_err(error)?,
        tenant_id: tenant.into(),
        object_version: row.try_get("object_version").map_err(error)?,
        table_catalog: table.table_catalog.clone(),
        schema_name: table.schema_name.clone(),
        table_name: table.table_name.clone(),
        source_fingerprint: row.try_get("source_fingerprint").map_err(error)?,
    };
    let properties: Value =
        serde_json::from_str(&row.try_get::<String, _>("properties").map_err(error)?)
            .map_err(|_| invalid("元数据属性无效"))?;
    Ok(binding == target.binding
        && binding.object_version == row.try_get::<i64, _>("version").map_err(error)?
        && table.columns == target.columns
        && binding.source_fingerprint == table.fingerprint()?
        && binding_matches(&binding, &properties)
        && properties["columns"] == json!(table.columns))
}
