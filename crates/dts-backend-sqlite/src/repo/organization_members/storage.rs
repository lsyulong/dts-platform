use super::super::product_resource::SqliteProductResourceRepo;
pub(super) use super::super::product_resource::helpers::map_error as error;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, repo::organization_members::*};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
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
        .map_err(|_| invalid("组织持久化时间无效"))
}
pub(super) fn decode_membership(row: &SqliteRow) -> Result<OrganizationMembership, BackendError> {
    Ok(OrganizationMembership {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        user_id: row.try_get("user_id").map_err(error)?,
        organization_id: row.try_get("organization_id").map_err(error)?,
        version: row.try_get("version").map_err(error)?,
        changed_by: row.try_get("changed_by").map_err(error)?,
        changed_at: parse_time(&row.try_get::<String, _>("changed_at").map_err(error)?)?,
    })
}
pub(super) fn decode_user(row: &SqliteRow) -> Result<OrganizationUser, BackendError> {
    Ok(OrganizationUser {
        id: row.try_get("user_id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        username: row.try_get("username").map_err(error)?,
        display_name: row.try_get("display_name").map_err(error)?,
        status: row.try_get("user_status").map_err(error)?,
    })
}
pub(super) async fn user(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
    active: bool,
) -> Result<OrganizationUser, BackendError> {
    let row=sqlx::query("SELECT id AS user_id,tenant_id,username,display_name,status AS user_status FROM \"user\" WHERE tenant_id=?1 AND id=?2")
        .bind(tenant).bind(id).fetch_optional(conn).await.map_err(error)?.ok_or_else(||invalid("成员用户不存在或不属于当前租户"))?;
    let user = decode_user(&row)?;
    if active && user.status != "active" {
        return Err(conflict("加入、调动或新负责人必须是有效成员账号"));
    }
    Ok(user)
}
pub(super) async fn membership(
    conn: &mut SqliteConnection,
    tenant: &str,
    user: &str,
) -> Result<Option<OrganizationMembership>, BackendError> {
    sqlx::query("SELECT * FROM organization_membership WHERE tenant_id=?1 AND user_id=?2")
        .bind(tenant)
        .bind(user)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode_membership)
        .transpose()
}
pub(super) async fn organization(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<ProductResource, BackendError> {
    SqliteProductResourceRepo::read_resource(conn, tenant, "organization", id)
        .await?
        .ok_or_else(|| BackendError::NotFound("组织不存在或不属于当前租户".into()))
}
pub(super) async fn available(
    conn: &mut SqliteConnection,
    tenant: &str,
    organization: &ProductResource,
) -> Result<(), BackendError> {
    let mut current = Some(organization.id.clone());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = current {
        if seen.len() >= 128 || !seen.insert(id.clone()) {
            return Err(conflict("组织层级过深或形成环"));
        }
        let resource = self::organization(conn, tenant, &id).await?;
        if !matches!(resource.status.as_str(), "draft" | "published") {
            return Err(conflict("目标组织或其父组织当前不可接收成员"));
        }
        current = resource.parent_id;
    }
    Ok(())
}
pub(super) async fn manager_id(
    conn: &mut SqliteConnection,
    tenant: &str,
    org: &str,
) -> Result<Option<String>, BackendError> {
    sqlx::query_scalar("SELECT manager_user_id FROM organization_manager WHERE tenant_id=?1 AND organization_id=?2")
        .bind(tenant).bind(org).fetch_optional(conn).await.map_err(error)
}
fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, BackendError> {
    if value.len() > 64 * 1024 {
        return Err(invalid("组织操作回执超过容量"));
    }
    serde_json::from_str(value).map_err(|_| invalid("组织操作回执损坏"))
}
pub(super) async fn replay(
    conn: &mut SqliteConnection,
    s: &OrganizationScope,
    key: &str,
    semantic: &Value,
) -> Result<Option<OrganizationDecisionResult>, BackendError> {
    let row=sqlx::query("SELECT actor_id,request_semantics,result_snapshot FROM organization_membership_receipt WHERE tenant_id=?1 AND idempotency_key=?2")
        .bind(&s.tenant_id).bind(key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    if row.try_get::<String, _>("actor_id").map_err(error)? != s.actor_id
        || parse::<Value>(
            &row.try_get::<String, _>("request_semantics")
                .map_err(error)?,
        )? != *semantic
    {
        return Err(conflict("组织幂等键已用于不同实际请求"));
    }
    let mut result: OrganizationDecisionResult =
        parse(&row.try_get::<String, _>("result_snapshot").map_err(error)?)?;
    result.replayed = true;
    Ok(Some(result))
}
pub(super) struct ReceiptWrite<'a> {
    pub scope: &'a OrganizationScope,
    pub key: &'a str,
    pub action: &'a str,
    pub reason: &'a str,
    pub user_id: Option<&'a str>,
    pub semantics: &'a Value,
    pub result: OrganizationDecisionResult,
}
pub(super) async fn receipt(
    conn: &mut SqliteConnection,
    p: ReceiptWrite<'_>,
) -> Result<OrganizationDecisionResult, BackendError> {
    let encoded = serde_json::to_string(&p.result).map_err(|_| invalid("组织回执无法编码"))?;
    if encoded.len() > 64 * 1024 {
        return Err(invalid("组织回执超过容量"));
    }
    sqlx::query("INSERT INTO organization_membership_receipt(id,tenant_id,idempotency_key,action,actor_id,user_id,source_organization_id,target_organization_id,reason,request_semantics,result_snapshot,created_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)")
        .bind(&p.result.receipt_id).bind(&p.scope.tenant_id).bind(p.key).bind(p.action).bind(&p.scope.actor_id).bind(p.user_id)
        .bind(p.result.source.as_ref().map(|o|&o.id)).bind(p.result.target.as_ref().map(|o|&o.id)).bind(p.reason).bind(p.semantics.to_string())
        .bind(encoded).bind(now(conn).await?.to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(p.result)
}
pub(super) fn decode_history(row: &SqliteRow) -> Result<OrganizationHistoryEntry, BackendError> {
    Ok(OrganizationHistoryEntry {
        id: row.try_get("id").map_err(error)?,
        action: row.try_get("action").map_err(error)?,
        actor_id: row.try_get("actor_id").map_err(error)?,
        user_id: row.try_get("user_id").map_err(error)?,
        reason: row.try_get("reason").map_err(error)?,
        result: parse(&row.try_get::<String, _>("result_snapshot").map_err(error)?)?,
        created_at: parse_time(&row.try_get::<String, _>("created_at").map_err(error)?)?,
    })
}
