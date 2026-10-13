// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::product_job::*};
use serde_json::Value;
use sqlx::{PgConnection, Row, postgres::PgRow};

pub(super) const COLUMNS: &str = "id, tenant_id, source_kind, source_id, source_version, actor_id,
action, idempotency_key, definition_snapshot::text AS definition_snapshot, parameters::text AS parameters, status, cancel_requested, cancel_requested_by,
can_retry, attempt, lease_owner, lease_fence, lease_expires_at, result::text AS result, error, reconciled_by,
created_at, updated_at, started_at, finished_at";

pub(super) fn error(error: sqlx::Error) -> BackendError {
    if let sqlx::Error::Database(db) = &error {
        if db.is_unique_violation() {
            return BackendError::Conflict("来源已有活动运行或幂等键已经使用".into());
        }
        if db.is_foreign_key_violation() {
            return BackendError::ConstraintViolation("运行来源或身份引用无效".into());
        }
        if db.code().as_deref() == Some("23514") {
            return BackendError::ConstraintViolation("运行状态、结果或记录约束无效".into());
        }
    }
    dts_backend_rdbms::map_sqlx_error(error)
}

fn json(value: &str) -> Result<Value, BackendError> {
    serde_json::from_str(value)
        .map_err(|e| BackendError::Internal(format!("运行存储 JSON 无效: {e}")))
}

fn job_state(value: String) -> Result<ProductJobStatus, BackendError> {
    serde_json::from_value(Value::String(value))
        .map_err(|e| BackendError::Internal(format!("运行存储状态无效: {e}")))
}

fn node_state(value: String) -> Result<ProductJobNodeStatus, BackendError> {
    serde_json::from_value(Value::String(value))
        .map_err(|e| BackendError::Internal(format!("节点存储状态无效: {e}")))
}

fn time(row: &PgRow, field: &str) -> Result<Option<DateTime<Utc>>, BackendError> {
    row.try_get(field).map_err(error)
}

fn required_time(row: &PgRow, field: &str) -> Result<DateTime<Utc>, BackendError> {
    time(row, field)?.ok_or_else(|| BackendError::Internal("运行时间缺失".into()))
}

fn optional_json(row: &PgRow, field: &str) -> Result<Option<Value>, BackendError> {
    row.try_get::<Option<String>, _>(field)
        .map_err(error)?
        .as_deref()
        .map(json)
        .transpose()
}

fn node(row: &PgRow) -> Result<ProductJobNode, BackendError> {
    Ok(ProductJobNode {
        id: row.try_get("node_id").map_err(error)?,
        name: row.try_get("name").map_err(error)?,
        definition: json(&row.try_get::<String, _>("definition").map_err(error)?)?,
        depends_on: serde_json::from_str(&row.try_get::<String, _>("depends_on").map_err(error)?)
            .map_err(|e| BackendError::Internal(format!("节点依赖 JSON 无效: {e}")))?,
        status: node_state(row.try_get("status").map_err(error)?)?,
        attempt: row.try_get("attempt").map_err(error)?,
        result: optional_json(row, "result")?,
        error: row.try_get("error").map_err(error)?,
        started_at: time(row, "started_at")?,
        finished_at: time(row, "finished_at")?,
    })
}

pub(super) async fn get(
    conn: &mut PgConnection,
    tenant_id: &str,
    job_id: &str,
    actor_id: Option<&str>,
) -> Result<Option<ProductJob>, BackendError> {
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM product_job
        WHERE tenant_id = $1 AND id = $2 AND ($3 IS NULL OR actor_id = $3)"
    ))
    .bind(tenant_id)
    .bind(job_id)
    .bind(actor_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let nodes = sqlx::query("SELECT node_id, name, definition::text AS definition, depends_on::text AS depends_on, status, attempt, result::text AS result, error,
        started_at, finished_at FROM product_job_node WHERE tenant_id = $1 AND job_id = $2 ORDER BY node_id")
        .bind(tenant_id).bind(job_id).fetch_all(&mut *conn).await.map_err(error)?
        .iter().map(node).collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ProductJob {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        source_kind: row.try_get("source_kind").map_err(error)?,
        source_id: row.try_get("source_id").map_err(error)?,
        source_version: row.try_get("source_version").map_err(error)?,
        actor_id: row.try_get("actor_id").map_err(error)?,
        action: row.try_get("action").map_err(error)?,
        idempotency_key: row.try_get("idempotency_key").map_err(error)?,
        definition_snapshot: json(
            &row.try_get::<String, _>("definition_snapshot")
                .map_err(error)?,
        )?,
        parameters: json(&row.try_get::<String, _>("parameters").map_err(error)?)?,
        status: job_state(row.try_get("status").map_err(error)?)?,
        cancel_requested: row.try_get("cancel_requested").map_err(error)?,
        cancel_requested_by: row.try_get("cancel_requested_by").map_err(error)?,
        can_retry: row.try_get("can_retry").map_err(error)?,
        attempt: row.try_get("attempt").map_err(error)?,
        lease_owner: row.try_get("lease_owner").map_err(error)?,
        lease_fence: row.try_get("lease_fence").map_err(error)?,
        lease_expires_at: time(&row, "lease_expires_at")?,
        result: optional_json(&row, "result")?,
        error: row.try_get("error").map_err(error)?,
        reconciled_by: row.try_get("reconciled_by").map_err(error)?,
        created_at: required_time(&row, "created_at")?,
        updated_at: required_time(&row, "updated_at")?,
        started_at: time(&row, "started_at")?,
        finished_at: time(&row, "finished_at")?,
        nodes,
    }))
}

pub(super) async fn required(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<ProductJob, BackendError> {
    get(conn, tenant, id, None)
        .await?
        .ok_or_else(|| BackendError::NotFound("运行不存在".into()))
}

pub(super) async fn locked(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<ProductJob, BackendError> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT id FROM product_job WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    if found.is_none() {
        return Err(BackendError::NotFound("运行不存在".into()));
    }
    required(conn, tenant, id).await
}

pub(super) async fn leased(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    lease: &ProductJobLease,
) -> Result<ProductJob, BackendError> {
    let valid: Option<String> = sqlx::query_scalar("SELECT id FROM product_job
        WHERE tenant_id = $1 AND id = $2 AND status = 'running' AND lease_owner = $3 AND lease_fence = $4
        AND lease_expires_at > clock_timestamp() FOR UPDATE")
        .bind(tenant).bind(id).bind(&lease.worker_id).bind(lease.fence)
        .fetch_optional(&mut *conn).await.map_err(error)?;
    if valid.is_none() {
        return Err(BackendError::Conflict(
            "运行租约失效或已被其他执行器接管".into(),
        ));
    }
    required(conn, tenant, id).await
}

pub(super) fn changed(affected: u64) -> Result<(), BackendError> {
    if affected != 1 {
        return Err(BackendError::Conflict("运行状态或租约已经变更".into()));
    }
    Ok(())
}
