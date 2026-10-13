// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_job::validation as validate, repo::product_job::*};
use sqlx::PgConnection;

use super::{PgProductJobRepo, read};

pub(super) async fn validate_actor(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
) -> Result<(), BackendError> {
    let active: Option<String> = sqlx::query_scalar(
        "SELECT id FROM \"user\" WHERE tenant_id = $1 AND id = $2 AND status = 'active'",
    )
    .bind(tenant)
    .bind(actor)
    .fetch_optional(conn)
    .await
    .map_err(read::error)?;
    if active.is_none() {
        return Err(BackendError::ConstraintViolation(
            "操作人不存在、租户不匹配或已停用".into(),
        ));
    }
    Ok(())
}

pub(super) async fn reap_expired(
    conn: &mut PgConnection,
    tenant: &str,
    kinds: &str,
) -> Result<(), BackendError> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM product_job WHERE tenant_id = $1
        AND source_kind IN (SELECT jsonb_array_elements_text($2::jsonb)) AND status = 'running'
        AND lease_expires_at <= clock_timestamp() AND (can_retry = FALSE OR cancel_requested = TRUE) FOR UPDATE SKIP LOCKED")
        .bind(tenant).bind(kinds).fetch_all(&mut *conn).await.map_err(read::error)?;
    for id in ids {
        let before = read::required(&mut *conn, tenant, &id).await?;
        let retryable_cancel = before.can_retry && before.cancel_requested;
        let status = if retryable_cancel {
            "cancelled"
        } else {
            "reconciliation_required"
        };
        sqlx::query(
            "UPDATE product_job SET status = $1, lease_owner = NULL, lease_expires_at = NULL,
            error = '执行器租约过期', updated_at = clock_timestamp(),
            finished_at = CASE WHEN $1 = 'cancelled' THEN clock_timestamp() ELSE NULL END
            WHERE tenant_id = $2 AND id = $3",
        )
        .bind(status)
        .bind(tenant)
        .bind(&id)
        .execute(&mut *conn)
        .await
        .map_err(read::error)?;
        let node_status = if retryable_cancel {
            "cancelled"
        } else {
            "reconciliation_required"
        };
        sqlx::query(
            "UPDATE product_job_node SET status = $1, error = '执行器租约过期',
            finished_at = CASE WHEN $1 = 'cancelled' THEN clock_timestamp() ELSE NULL END
            WHERE tenant_id = $2 AND job_id = $3 AND
            (status = 'running' OR ($1 = 'cancelled' AND status = 'pending'))",
        )
        .bind(node_status)
        .bind(tenant)
        .bind(&id)
        .execute(&mut *conn)
        .await
        .map_err(read::error)?;
    }
    Ok(())
}

pub(super) async fn cancel(
    repo: &PgProductJobRepo,
    params: &CancelProductJobParams,
) -> Result<ProductJob, BackendError> {
    let mut tx = repo.begin_write().await?;
    validate_actor(&mut tx, &params.tenant_id, &params.actor_id).await?;
    let before = read::locked(&mut tx, &params.tenant_id, &params.job_id).await?;
    if before.status == ProductJobStatus::Cancelled || before.cancel_requested {
        tx.commit().await.map_err(read::error)?;
        return Ok(before);
    }
    if !matches!(
        before.status,
        ProductJobStatus::Queued
            | ProductJobStatus::Running
            | ProductJobStatus::ReconciliationRequired
    ) {
        return Err(BackendError::Conflict("已结束的运行不能取消".into()));
    }
    let status = if before.status == ProductJobStatus::Queued {
        "cancelled"
    } else if before.status == ProductJobStatus::Running {
        "running"
    } else {
        "reconciliation_required"
    };
    sqlx::query(
        "UPDATE product_job SET cancel_requested = TRUE, cancel_requested_by = $1, status = $2,
        updated_at = clock_timestamp(),
        finished_at = CASE WHEN $2 = 'cancelled' THEN clock_timestamp() ELSE NULL END
        WHERE tenant_id = $3 AND id = $4",
    )
    .bind(&params.actor_id)
    .bind(status)
    .bind(&params.tenant_id)
    .bind(&params.job_id)
    .execute(&mut *tx)
    .await
    .map_err(read::error)?;
    if status == "cancelled" {
        sqlx::query("UPDATE product_job_node SET status = 'cancelled',
            finished_at = clock_timestamp() WHERE tenant_id = $1 AND job_id = $2 AND status = 'pending'")
            .bind(&params.tenant_id).bind(&params.job_id).execute(&mut *tx).await.map_err(read::error)?;
    }
    let job = read::required(&mut tx, &params.tenant_id, &params.job_id).await?;
    tx.commit().await.map_err(read::error)?;
    Ok(job)
}

pub(super) async fn reconcile(
    repo: &PgProductJobRepo,
    params: &ReconcileProductJobParams,
) -> Result<ProductJob, BackendError> {
    validate::bounded_text(&params.reason, 4000)?;
    let mut tx = repo.begin_write().await?;
    validate_actor(&mut tx, &params.tenant_id, &params.actor_id).await?;
    let before = read::locked(&mut tx, &params.tenant_id, &params.job_id).await?;
    if before.status != ProductJobStatus::ReconciliationRequired
        || before.lease_fence != params.expected_fence
    {
        return Err(BackendError::Conflict(
            "运行不处于待核对状态或版本已变更".into(),
        ));
    }
    let status = match params.outcome {
        ProductJobReconciliation::Failed => "failed",
        ProductJobReconciliation::Cancelled => "cancelled",
    };
    sqlx::query(
        "UPDATE product_job SET status = $1, error = $2, reconciled_by = $3,
        finished_at = clock_timestamp(), updated_at = clock_timestamp()
        WHERE tenant_id = $4 AND id = $5",
    )
    .bind(status)
    .bind(&params.reason)
    .bind(&params.actor_id)
    .bind(&params.tenant_id)
    .bind(&params.job_id)
    .execute(&mut *tx)
    .await
    .map_err(read::error)?;
    sqlx::query("UPDATE product_job_node SET status = CASE WHEN status = 'pending' AND $1 = 'failed' THEN 'skipped' ELSE $1 END,
        error = $2, finished_at = clock_timestamp()
        WHERE tenant_id = $3 AND job_id = $4 AND status IN ('pending', 'running', 'reconciliation_required')")
        .bind(status).bind(&params.reason).bind(&params.tenant_id).bind(&params.job_id)
        .execute(&mut *tx).await.map_err(read::error)?;
    let job = read::required(&mut tx, &params.tenant_id, &params.job_id).await?;
    tx.commit().await.map_err(read::error)?;
    Ok(job)
}
