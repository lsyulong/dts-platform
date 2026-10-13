pub(super) use super::super::product_resource::helpers::map_error as error;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::navigation::*};
use serde_json::Value;
use sqlx::{PgConnection, Row, postgres::PgRow};
pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub(super) async fn now(conn: &mut PgConnection) -> Result<DateTime<Utc>, BackendError> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(conn)
        .await
        .map_err(error)
}

fn parse<T: serde::de::DeserializeOwned>(encoded: &str) -> Result<T, BackendError> {
    if encoded.len() > 2 * 1024 * 1024 {
        return Err(invalid("导航冻结快照超过容量"));
    }
    serde_json::from_str(encoded).map_err(|_| invalid("导航冻结快照损坏"))
}
fn decode(row: &PgRow) -> Result<NavigationAssignment, BackendError> {
    let template: NavigationTemplateSnapshot = parse(
        &row.try_get::<String, _>("template_snapshot")
            .map_err(error)?,
    )?;
    if template.id != row.try_get::<String, _>("template_id").map_err(error)?
        || template.version != row.try_get::<i64, _>("template_version").map_err(error)?
    {
        return Err(conflict("导航实际模板身份和快照不一致"));
    }
    let status = match row.try_get::<String, _>("status").map_err(error)?.as_str() {
        "active" => NavigationAssignmentStatus::Active,
        "revoked" => NavigationAssignmentStatus::Revoked,
        _ => return Err(invalid("导航实际状态无效")),
    };
    Ok(NavigationAssignment {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        version: row.try_get("version").map_err(error)?,
        status,
        template,
        assigned_by: row.try_get("assigned_by").map_err(error)?,
        assigned_at: row.try_get("assigned_at").map_err(error)?,
        expires_at: row.try_get("expires_at").map_err(error)?,
        revoked_by: row.try_get("revoked_by").map_err(error)?,
        revoked_at: row.try_get("revoked_at").map_err(error)?,
    })
}
pub(super) async fn load(
    conn: &mut PgConnection,
    tenant: &str,
    lock: bool,
) -> Result<Option<NavigationAssignment>, BackendError> {
    let query = if lock {
        "SELECT * FROM tenant_navigation_assignment WHERE tenant_id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM tenant_navigation_assignment WHERE tenant_id=$1"
    };
    sqlx::query(query)
        .bind(tenant)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode)
        .transpose()
}
pub(super) async fn replay(
    conn: &mut PgConnection,
    scope: &NavigationScope,
    key: &str,
    semantics: &Value,
) -> Result<Option<NavigationDecisionResult>, BackendError> {
    let row=sqlx::query("SELECT id,actor_id,request_semantics,result_snapshot FROM tenant_navigation_receipt WHERE tenant_id=$1 AND idempotency_key=$2 FOR SHARE")
        .bind(&scope.tenant_id).bind(key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let original: Value = parse(
        &row.try_get::<String, _>("request_semantics")
            .map_err(error)?,
    )?;
    if original != *semantics
        || row.try_get::<String, _>("actor_id").map_err(error)? != scope.actor_id
    {
        return Err(conflict("导航幂等键已用于不同实际请求"));
    }
    Ok(Some(NavigationDecisionResult {
        assignment: parse(&row.try_get::<String, _>("result_snapshot").map_err(error)?)?,
        receipt_id: row.try_get("id").map_err(error)?,
        replayed: true,
    }))
}
pub(super) struct ReceiptWrite<'a> {
    pub scope: &'a NavigationScope,
    pub key: &'a str,
    pub action: &'a str,
    pub semantics: &'a Value,
    pub assignment: NavigationAssignment,
}
pub(super) async fn receipt(
    conn: &mut PgConnection,
    p: ReceiptWrite<'_>,
) -> Result<NavigationDecisionResult, BackendError> {
    let ReceiptWrite {
        scope,
        key,
        action,
        semantics,
        assignment,
    } = p;
    let id = uuid::Uuid::new_v4().to_string();
    let encoded = serde_json::to_string(&assignment).map_err(|_| invalid("导航回执无法编码"))?;
    if encoded.len() > 2 * 1024 * 1024 {
        return Err(invalid("导航回执超过容量"));
    }
    let observed = now(conn).await?;
    sqlx::query("INSERT INTO tenant_navigation_receipt(id,tenant_id,assignment_id,assignment_version,idempotency_key,action,actor_id,template_id,request_semantics,result_snapshot,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(&id).bind(&scope.tenant_id).bind(&assignment.id).bind(assignment.version).bind(key).bind(action)
        .bind(&scope.actor_id).bind(&assignment.template.id).bind(semantics.to_string()).bind(encoded)
        .bind(observed).execute(&mut *conn).await.map_err(error)?;
    for menu in &assignment.template.menus {
        sqlx::query("INSERT INTO tenant_navigation_frozen_menu(tenant_id,receipt_id,menu_id,menu_version) VALUES($1,$2,$3,$4)")
            .bind(&scope.tenant_id).bind(&id).bind(&menu.id).bind(menu.version).execute(&mut *conn).await.map_err(error)?;
    }
    Ok(NavigationDecisionResult {
        assignment,
        receipt_id: id,
        replayed: false,
    })
}
pub(super) async fn history(
    conn: &mut PgConnection,
    filter: &NavigationHistoryFilter,
) -> Result<NavigationHistoryPage, BackendError> {
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tenant_navigation_receipt WHERE tenant_id=$1")
            .bind(&filter.scope.tenant_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(error)?;
    let rows=sqlx::query("SELECT * FROM tenant_navigation_receipt WHERE tenant_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2 OFFSET $3")
        .bind(&filter.scope.tenant_id).bind(i64::from(filter.limit)).bind(i64::from(filter.offset)).fetch_all(conn).await.map_err(error)?;
    let data = rows
        .iter()
        .map(|row| {
            Ok(NavigationHistoryEntry {
                id: row.try_get("id").map_err(error)?,
                assignment_id: row.try_get("assignment_id").map_err(error)?,
                action: row.try_get("action").map_err(error)?,
                actor_id: row.try_get("actor_id").map_err(error)?,
                assignment_version: row.try_get("assignment_version").map_err(error)?,
                result: parse(&row.try_get::<String, _>("result_snapshot").map_err(error)?)?,
                created_at: row.try_get("created_at").map_err(error)?,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    Ok(NavigationHistoryPage { data, total })
}
