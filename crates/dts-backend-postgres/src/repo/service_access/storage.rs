use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, ResourceWrite};
use serde_json::Value;
use sqlx::PgConnection;

use super::super::product_resource::{PgProductResourceRepo as Resources, helpers};

pub(super) use helpers::map_error as error;

pub(super) fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}

pub(super) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    Resources::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("服务应用或订阅不存在".into()))
}

pub(super) async fn now(conn: &mut PgConnection) -> Result<DateTime<Utc>, BackendError> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(conn)
        .await
        .map_err(error)
}

pub(super) fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, BackendError> {
    DateTime::from_timestamp_micros(value.timestamp_micros())
        .ok_or_else(|| invalid("服务时间戳超出数据库范围"))
}

pub(super) struct WriteScope<'a> {
    pub tenant: &'a str,
    pub actor: &'a str,
    pub kind: &'a str,
    pub id: &'a str,
    pub operation: &'a str,
}

pub(super) async fn write(
    conn: &mut PgConnection,
    scope: &WriteScope<'_>,
    fields: &ResourceWrite,
    before: Option<&ProductResource>,
) -> Result<ProductResource, BackendError> {
    helpers::validate_graph(conn, scope.tenant, scope.kind, scope.id, fields).await?;
    let encoded = fields.properties.to_string();
    if encoded.len() > 256 * 1024 {
        return Err(invalid("服务定义超过256KiB"));
    }
    let version = before.map_or(Ok(1), |r| {
        r.version
            .checked_add(1)
            .ok_or_else(|| conflict("服务版本已达上限"))
    })?;
    let now = now(conn).await?;
    if let Some(before) = before {
        let changed=sqlx::query("UPDATE product_resource SET code=$1,name=$2,description=$3,status=$4,properties=$5::jsonb,version=$6,updated_at=$7
            WHERE tenant_id=$8 AND kind=$9 AND id=$10 AND version=$11")
            .bind(&fields.code).bind(&fields.name).bind(&fields.description).bind(&fields.status).bind(encoded).bind(version).bind(now)
            .bind(scope.tenant).bind(scope.kind).bind(scope.id).bind(before.version).execute(&mut *conn).await.map_err(error)?.rows_affected();
        if changed != 1 {
            return Err(conflict("服务定义已经变更"));
        }
    } else {
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,description,status,properties,version,created_by,created_at,updated_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,1,$9,$10,$10)")
            .bind(scope.id).bind(scope.tenant).bind(scope.kind).bind(&fields.code).bind(&fields.name).bind(&fields.description)
            .bind(&fields.status).bind(encoded).bind(scope.actor).bind(now).execute(&mut *conn).await.map_err(error)?;
    }
    Resources::write_references(conn, scope.tenant, scope.kind, scope.id, &fields.references)
        .await?;
    let saved = get(conn, scope.tenant, scope.kind, scope.id).await?;
    helpers::audit(conn, scope.actor, scope.operation, before, Some(&saved)).await?;
    Ok(saved)
}

pub(super) fn fields(resource: &ProductResource, status: &str, properties: Value) -> ResourceWrite {
    ResourceWrite {
        code: resource.code.clone(),
        name: resource.name.clone(),
        description: resource.description.clone(),
        parent_id: None,
        status: status.into(),
        properties,
        references: resource.references.clone(),
    }
}

pub(super) fn version(resource: &ProductResource, expected: i64) -> Result<(), BackendError> {
    if resource.version != expected {
        return Err(conflict("服务定义版本已改变，请刷新重试"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_precision_matches_postgres_without_extending_deadline() {
        let original = DateTime::parse_from_rfc3339("2027-03-04T05:06:07.123456789Z")
            .expect("time")
            .with_timezone(&Utc);
        let canonical = canonical_time(original).expect("canonical");
        assert_eq!(canonical.timestamp_subsec_nanos(), 123456000);
        assert!(canonical <= original);
        let properties = serde_json::json!({"expires_at":canonical});
        let decoded: DateTime<Utc> =
            serde_json::from_value(properties["expires_at"].clone()).expect("JSON time");
        assert_eq!(canonical, decoded);
    }
}
