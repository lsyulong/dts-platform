use super::super::product_resource::SqliteProductResourceRepo;
pub(super) use super::super::product_resource::helpers::map_error as error;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, repo::development_revisions::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) async fn now(conn: &mut SqliteConnection) -> Result<DateTime<Utc>, BackendError> {
    let text: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')")
        .fetch_one(conn)
        .await
        .map_err(error)?;
    parse_time(&text)
}
fn parse_time(text: &str) -> Result<DateTime<Utc>, BackendError> {
    DateTime::parse_from_rfc3339(text)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| BackendError::Internal("修订持久化时间无效".into()))
}
pub(super) async fn source(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
) -> Result<ProductResource, BackendError> {
    SqliteProductResourceRepo::read_resource(conn, &s.tenant_id, &s.kind, &s.resource_id)
        .await?
        .ok_or_else(|| BackendError::NotFound("开发定义不存在或不属于当前租户".into()))
}
pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, BackendError> {
    serde_json::to_string(value).map_err(|_| BackendError::Internal("修订记录无法编码".into()))
}
pub(super) fn decode<T: DeserializeOwned>(text: &str) -> Result<T, BackendError> {
    serde_json::from_str(text).map_err(|_| BackendError::Internal("修订持久化 JSON 无效".into()))
}
pub(super) fn decode_summary(row: &SqliteRow) -> Result<DevelopmentRevisionSummary, BackendError> {
    Ok(DevelopmentRevisionSummary {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        source_kind: row.try_get("source_kind").map_err(error)?,
        source_id: row.try_get("source_id").map_err(error)?,
        source_version: row.try_get("source_version").map_err(error)?,
        snapshot_hash: row.try_get("snapshot_hash").map_err(error)?,
        saved_by: row.try_get("saved_by").map_err(error)?,
        saved_at: parse_time(&row.try_get::<String, _>("saved_at").map_err(error)?)?,
        reason: row.try_get("reason").map_err(error)?,
    })
}
fn decode_revision(row: &SqliteRow) -> Result<DevelopmentRevision, BackendError> {
    let summary = decode_summary(row)?;
    let snapshot: ProductResource = decode(&row.try_get::<String, _>("snapshot").map_err(error)?)?;
    if snapshot.tenant_id != summary.tenant_id
        || snapshot.kind != summary.source_kind
        || snapshot.id != summary.source_id
        || snapshot.version != summary.source_version
        || validation::snapshot_hash(&snapshot)? != summary.snapshot_hash
    {
        return Err(BackendError::Internal(
            "不可变修订快照完整性校验失败".into(),
        ));
    }
    Ok(DevelopmentRevision { summary, snapshot })
}
pub(super) async fn revision(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
    id: &str,
) -> Result<DevelopmentRevision, BackendError> {
    let row=sqlx::query("SELECT * FROM development_revision WHERE tenant_id=?1 AND source_kind=?2 AND source_id=?3 AND id=?4")
        .bind(&s.tenant_id).bind(&s.kind).bind(&s.resource_id).bind(id).fetch_optional(conn).await.map_err(error)?
        .ok_or_else(||BackendError::NotFound("该开发定义的修订不存在".into()))?;
    decode_revision(&row)
}
pub(super) async fn version_revision(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
    version: i64,
) -> Result<Option<DevelopmentRevision>, BackendError> {
    let row=sqlx::query("SELECT * FROM development_revision WHERE tenant_id=?1 AND source_kind=?2 AND source_id=?3 AND source_version=?4")
        .bind(&s.tenant_id).bind(&s.kind).bind(&s.resource_id).bind(version).fetch_optional(conn).await.map_err(error)?;
    row.as_ref().map(decode_revision).transpose()
}
pub(super) async fn replay<T: DeserializeOwned>(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
    key: &str,
    action: &str,
    semantic: &Value,
) -> Result<Option<T>, BackendError> {
    let row=sqlx::query("SELECT actor_id,action,semantic,result_snapshot FROM development_revision_receipt WHERE tenant_id=?1 AND idempotency_key=?2")
        .bind(&s.tenant_id).bind(key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    if row.try_get::<String, _>("actor_id").map_err(error)? != s.actor_id
        || row.try_get::<String, _>("action").map_err(error)? != action
        || decode::<Value>(&row.try_get::<String, _>("semantic").map_err(error)?)? != *semantic
    {
        return Err(conflict("稳定幂等键已用于另一修订操作"));
    }
    Ok(Some(decode(
        &row.try_get::<String, _>("result_snapshot").map_err(error)?,
    )?))
}
pub(super) struct Receipt<'a, T> {
    pub id: &'a str,
    pub key: &'a str,
    pub action: &'a str,
    pub revision_id: &'a str,
    pub semantic: &'a Value,
    pub result: &'a T,
    pub at: DateTime<Utc>,
}
pub(super) async fn receipt<T: Serialize>(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
    r: Receipt<'_, T>,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO development_revision_receipt(id,tenant_id,idempotency_key,actor_id,action,revision_id,semantic,result_snapshot,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(r.id).bind(&s.tenant_id).bind(r.key).bind(&s.actor_id).bind(r.action).bind(r.revision_id).bind(encode(r.semantic)?).bind(encode(r.result)?).bind(r.at.to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(())
}
pub(super) async fn audit(
    conn: &mut SqliteConnection,
    s: &DevelopmentRevisionScope,
    revision: &DevelopmentRevisionSummary,
    after: i64,
    at: DateTime<Utc>,
    action: &str,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,?4,?5,'update',?6,?7)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&s.tenant_id).bind(&s.actor_id).bind(&s.kind).bind(&s.resource_id)
        .bind(json!({"action":action,"revision_id":revision.id,"snapshot_hash":revision.snapshot_hash,"source_version":revision.source_version,"result_version":after}).to_string())
        .bind(at.to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(())
}
