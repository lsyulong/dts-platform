// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::bi_share::*};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};
pub(super) fn error(error: sqlx::Error) -> BackendError {
    if let sqlx::Error::Database(db) = &error {
        if db.is_unique_violation() {
            return BackendError::Conflict("分享标识已经存在".into());
        }
        if db.is_foreign_key_violation() || db.code().as_deref() == Some("275") {
            return BackendError::ConstraintViolation(
                "分享租户、真实用户、结果或状态关联无效".into(),
            );
        }
    }
    dts_backend_rdbms::map_sqlx_error(error)
}
pub(super) fn invalid(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) async fn authority(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    write: bool,
) -> Result<(), BackendError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.id=?1 AND t.id=?2 AND u.status='active' AND t.status='active')")
        .bind(actor).bind(tenant).fetch_one(&mut *conn).await.map_err(error)?;
    if !active {
        return Err(invalid("分享真实用户或租户已停用"));
    }
    let permissions:Vec<String>=sqlx::query_scalar("SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=?1 AND r.tenant_id=?2")
        .bind(actor).bind(tenant).fetch_all(conn).await.map_err(error)?;
    if !permissions.iter().any(|p| p == "bi:read")
        || (write && !permissions.iter().any(|p| p == "bi:write"))
    {
        return Err(invalid("分享真实用户缺少当前精确BI权限"));
    }
    Ok(())
}
fn time(row: &SqliteRow, key: &str) -> Result<DateTime<Utc>, BackendError> {
    let value: String = row.try_get(key).map_err(error)?;
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| BackendError::Internal("分享持久化时间格式无效".into()))
}
fn map(row: SqliteRow) -> Result<BiShare, BackendError> {
    let status: String = row.try_get("status").map_err(error)?;
    let revoked: Option<String> = row.try_get("revoked_at").map_err(error)?;
    Ok(BiShare {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        report_id: row.try_get("report_id").map_err(error)?,
        report_version: row.try_get("report_version").map_err(error)?,
        job_id: row.try_get("job_id").map_err(error)?,
        node_id: row.try_get("node_id").map_err(error)?,
        owner_id: row.try_get("owner_id").map_err(error)?,
        recipient_id: row.try_get("recipient_id").map_err(error)?,
        status: match status.as_str() {
            "active" => BiShareStatus::Active,
            "revoked" => BiShareStatus::Revoked,
            _ => return Err(BackendError::Internal("分享存储状态无效".into())),
        },
        version: row.try_get("version").map_err(error)?,
        expires_at: time(&row, "expires_at")?,
        is_expired: row.try_get("is_expired").map_err(error)?,
        created_at: time(&row, "created_at")?,
        revoked_at: revoked.map(|_| time(&row, "revoked_at")).transpose()?,
    })
}
pub(super) async fn get(
    conn: &mut SqliteConnection,
    lookup: &BiShareLookup,
) -> Result<Option<BiShare>, BackendError> {
    sqlx::query("SELECT *, julianday(expires_at)<=julianday('now') AS is_expired FROM bi_share WHERE tenant_id=?1 AND id=?2 AND (owner_id=?3 OR recipient_id=?3)")
        .bind(&lookup.tenant_id).bind(&lookup.share_id).bind(&lookup.actor_id).fetch_optional(conn).await.map_err(error)?.map(map).transpose()
}
pub(super) async fn required(
    conn: &mut SqliteConnection,
    lookup: &BiShareLookup,
) -> Result<BiShare, BackendError> {
    get(conn, lookup)
        .await?
        .ok_or_else(|| BackendError::NotFound("分享不属于当前租户的真实参与用户".into()))
}
pub(super) async fn audit(
    conn: &mut SqliteConnection,
    share: &BiShare,
    actor: &str,
    operation: &str,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO bi_share_audit(id,tenant_id,share_id,actor_id,operation) VALUES(?1,?2,?3,?4,?5)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&share.tenant_id).bind(&share.id).bind(actor).bind(operation).execute(conn).await.map_err(error)?;
    Ok(())
}
