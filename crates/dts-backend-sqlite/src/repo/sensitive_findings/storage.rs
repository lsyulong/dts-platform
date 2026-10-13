// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::super::product_resource::SqliteProductResourceRepo;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, repo::sensitive_findings::*};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
pub(super) fn error(value: sqlx::Error) -> BackendError {
    super::super::product_resource::map_reference_error(value)
}
pub(super) fn invalid(value: &str) -> BackendError {
    BackendError::Conflict(value.into())
}
pub(super) async fn authority(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    action: &str,
) -> Result<(), BackendError> {
    let active:Option<String>=sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.id=?1 AND t.id=?2 AND u.status='active' AND t.status='active'").bind(actor).bind(tenant).fetch_optional(&mut *conn).await.map_err(error)?;
    let permissions:Vec<String>=sqlx::query_scalar("SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2").bind(actor).bind(tenant).fetch_all(&mut *conn).await.map_err(error)?;
    if active.is_none()
        || !permissions.iter().any(|p| p == "sensitive_data:read")
        || (action != "read"
            && !permissions
                .iter()
                .any(|p| p == &format!("sensitive_data:{action}")))
    {
        return Err(BackendError::Forbidden(
            "敏感识别操作人或当前精确权限已失效".into(),
        ));
    }
    Ok(())
}
pub(super) async fn resource(
    conn: &mut SqliteConnection,
    tenant: &str,
    kind: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    SqliteProductResourceRepo::read_resource(conn, tenant, kind, id)
        .await?
        .ok_or_else(|| invalid("敏感识别关联资源已不存在"))
}
pub(super) async fn live_metadata(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
    version: i64,
    fingerprint: &str,
) -> Result<(), BackendError> {
    let valid:Option<String>=sqlx::query_scalar("SELECT c.object_id FROM metadata_catalog c JOIN product_resource r ON r.tenant_id=c.tenant_id AND r.kind=c.object_kind AND r.id=c.object_id WHERE c.tenant_id=?1 AND c.object_id=?2 AND c.object_version=?3 AND r.version=?3 AND c.engine='swanlake' AND c.discovery_status='present' AND r.status='published' AND c.table_catalog=json_extract(r.properties,'$.table_catalog') AND c.schema_name=json_extract(r.properties,'$.schema_name') AND c.table_name=json_extract(r.properties,'$.table_name') AND json(c.columns_snapshot)=json(json_extract(r.properties,'$.columns')) AND c.source_fingerprint=?4 AND json_extract(r.properties,'$.source_fingerprint')=?4 AND json_extract(r.properties,'$.engine_binding.tenant_id')=?1 AND json_extract(r.properties,'$.engine_binding.engine')='swanlake'").bind(tenant).bind(id).bind(version).bind(fingerprint).fetch_optional(conn).await.map_err(error)?;
    if valid.is_none() {
        return Err(invalid("敏感识别对象的内部采集证明已失效"));
    }
    Ok(())
}
fn timestamp(value: String) -> Result<DateTime<Utc>, BackendError> {
    DateTime::parse_from_rfc3339(&value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| BackendError::Internal("发现记录时间无效".into()))
}
fn decode(row: SqliteRow) -> Result<SensitiveFinding, BackendError> {
    Ok(SensitiveFinding {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        task_id: row.try_get("task_id").map_err(error)?,
        task_version: row.try_get("task_version").map_err(error)?,
        job_id: row.try_get("job_id").map_err(error)?,
        node_id: row.try_get("node_id").map_err(error)?,
        object_id: row.try_get("object_id").map_err(error)?,
        object_version: row.try_get("object_version").map_err(error)?,
        rule_id: row.try_get("rule_id").map_err(error)?,
        rule_version: row.try_get("rule_version").map_err(error)?,
        classification_id: row.try_get("classification_id").map_err(error)?,
        classification_version: row.try_get("classification_version").map_err(error)?,
        column_name: row.try_get("column_name").map_err(error)?,
        evidence: serde_json::from_str(&row.try_get::<String, _>("evidence").map_err(error)?)
            .map_err(|_| invalid("发现实际证据格式无效"))?,
        evaluation_scope: row.try_get("evaluation_scope").map_err(error)?,
        sampled_rows: row
            .try_get::<i64, _>("sampled_rows")
            .map_err(error)?
            .try_into()
            .map_err(|_| invalid("真实取样行数无效"))?,
        definition_hash: row.try_get("definition_hash").map_err(error)?,
        status: row.try_get("status").map_err(error)?,
        version: row.try_get("version").map_err(error)?,
        reviewed_by: row.try_get("reviewed_by").map_err(error)?,
        review_reason: row.try_get("review_reason").map_err(error)?,
        created_at: timestamp(row.try_get("created_at").map_err(error)?)?,
        reviewed_at: row
            .try_get::<Option<String>, _>("reviewed_at")
            .map_err(error)?
            .map(timestamp)
            .transpose()?,
    })
}
pub(super) async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<SensitiveFinding>, BackendError> {
    sqlx::query("SELECT f.* FROM sensitive_finding f JOIN product_job j ON j.tenant_id=f.tenant_id AND j.id=f.job_id JOIN product_job_node n ON n.tenant_id=f.tenant_id AND n.job_id=f.job_id AND n.node_id=f.node_id WHERE f.tenant_id=?1 AND f.id=?2 AND j.status='succeeded' AND n.status='succeeded'").bind(tenant).bind(id).fetch_optional(conn).await.map_err(error)?.map(decode).transpose()
}
pub(super) async fn required(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<SensitiveFinding, BackendError> {
    get(conn, tenant, id)
        .await?
        .ok_or_else(|| BackendError::NotFound("真实已成功识别产生的发现不存在".into()))
}
pub(super) async fn audit(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
    actor: &str,
    operation: &str,
    reason: Option<&str>,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO sensitive_finding_audit(id,tenant_id,finding_id,actor_id,operation,reason) VALUES(?1,?2,?3,?4,?5,?6)").bind(uuid::Uuid::new_v4().to_string()).bind(tenant).bind(id).bind(actor).bind(operation).bind(reason).execute(conn).await.map_err(error)?;
    Ok(())
}
pub(super) async fn resource_audit(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
    actor: &str,
    operation: &str,
    summary: serde_json::Value,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,'sensitive_finding',?4,?5,?6,strftime('%Y-%m-%dT%H:%M:%fZ','now'))").bind(uuid::Uuid::new_v4().to_string()).bind(tenant).bind(actor).bind(id).bind(operation).bind(summary.to_string()).execute(conn).await.map_err(error)?;
    Ok(())
}
