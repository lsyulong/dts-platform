// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_job::validation as validate, repo::product_job::*};

use super::{SqliteProductJobRepo, control, read};

pub(super) async fn enqueue(
    repo: &SqliteProductJobRepo,
    params: &EnqueueProductJobParams,
) -> Result<EnqueuedProductJob, BackendError> {
    let encoded = validate::enqueue(params)?;
    let mut tx = repo.begin_write().await?;
    let configuration =
        super::super::runtime_configuration::jobs::verify_enqueue(&mut tx, params).await?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM product_job WHERE tenant_id = ?1 AND idempotency_key = ?2",
    )
    .bind(&params.tenant_id)
    .bind(&params.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(read::error)?;
    if let Some(id) = existing {
        let job = read::required(&mut tx, &params.tenant_id, &id).await?;
        if !validate::same_request(&job, params) {
            return Err(BackendError::Conflict("幂等键已用于其他运行请求".into()));
        }
        tx.commit().await.map_err(read::error)?;
        return Ok(EnqueuedProductJob {
            job,
            created: false,
        });
    }
    validate_source(&mut tx, params).await?;
    sqlx::query(
        "INSERT INTO product_job (id, tenant_id, source_kind, source_id, source_version, actor_id,
        action, idempotency_key, definition_snapshot, parameters, status, can_retry)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11)",
    )
    .bind(&params.id)
    .bind(&params.tenant_id)
    .bind(&params.source_kind)
    .bind(&params.source_id)
    .bind(params.source_version)
    .bind(&params.actor_id)
    .bind(&params.action)
    .bind(&params.idempotency_key)
    .bind(encoded.definition_snapshot)
    .bind(encoded.parameters)
    .bind(params.can_retry)
    .execute(&mut *tx)
    .await
    .map_err(read::error)?;
    for node in encoded.nodes {
        sqlx::query("INSERT INTO product_job_node (tenant_id, job_id, node_id, name, definition, depends_on)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
            .bind(&params.tenant_id).bind(&params.id).bind(node.id).bind(node.name)
            .bind(node.definition).bind(node.depends_on).execute(&mut *tx).await.map_err(read::error)?;
    }
    let job = read::required(&mut tx, &params.tenant_id, &params.id).await?;
    super::super::runtime_configuration::jobs::register_job(
        &mut tx,
        &params.tenant_id,
        &params.id,
        configuration.as_ref(),
    )
    .await?;
    tx.commit().await.map_err(read::error)?;
    Ok(EnqueuedProductJob { job, created: true })
}

async fn validate_source(
    conn: &mut sqlx::SqliteConnection,
    params: &EnqueueProductJobParams,
) -> Result<(), BackendError> {
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM product_resource WHERE tenant_id = ?1 AND kind = ?2 AND id = ?3",
    )
    .bind(&params.tenant_id)
    .bind(&params.source_kind)
    .bind(&params.source_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(read::error)?;
    if version != Some(params.source_version) {
        return Err(BackendError::Conflict(
            "来源不存在、租户不匹配或版本已变更".into(),
        ));
    }
    let actor: Option<String> = sqlx::query_scalar(
        "SELECT id FROM \"user\" WHERE tenant_id = ?1 AND id = ?2 AND status = 'active'",
    )
    .bind(&params.tenant_id)
    .bind(&params.actor_id)
    .fetch_optional(conn)
    .await
    .map_err(read::error)?;
    if actor.is_none() {
        return Err(BackendError::ConstraintViolation(
            "运行提交者不存在或已停用".into(),
        ));
    }
    Ok(())
}

pub(super) async fn claim(
    repo: &SqliteProductJobRepo,
    params: &ClaimProductJobParams,
) -> Result<Option<ProductJob>, BackendError> {
    let seconds = validate::lease_seconds(params.lease_seconds)?;
    validate::bounded_text(&params.worker_id, 128)?;
    if params.source_kinds.is_empty() || params.source_kinds.len() > 64 {
        return Err(BackendError::ConstraintViolation(
            "执行器必须指定来源类型白名单".into(),
        ));
    }
    for kind in &params.source_kinds {
        validate::bounded_text(kind, 128)?;
    }
    let kinds = serde_json::to_string(&params.source_kinds)
        .map_err(|e| BackendError::Internal(e.to_string()))?;
    let mut tx = repo.begin_write().await?;
    control::reap_expired(&mut tx, &params.tenant_id, &kinds).await?;
    let claim_sql = format!(
        "SELECT id FROM product_job WHERE tenant_id = ?1
        AND source_kind IN (SELECT value FROM json_each(?2))
        AND (status = 'queued' OR (status = 'running' AND can_retry = 1 AND cancel_requested = 0
            AND julianday(lease_expires_at) <= julianday('now')))
        {} ORDER BY created_at, id LIMIT 1",
        super::super::runtime_configuration::jobs::CLAIM_CAPACITY
    );
    let id: Option<String> = sqlx::query_scalar(&claim_sql)
        .bind(&params.tenant_id)
        .bind(&kinds)
        .fetch_optional(&mut *tx)
        .await
        .map_err(read::error)?;
    let Some(id) = id else {
        tx.commit().await.map_err(read::error)?;
        return Ok(None);
    };
    let before = read::required(&mut tx, &params.tenant_id, &id).await?;
    if before.lease_fence == i64::MAX || before.attempt == i64::MAX {
        return Err(BackendError::Conflict("运行租约计数已达上限".into()));
    }
    let affected = sqlx::query("UPDATE product_job SET status = 'running', lease_owner = ?1, lease_fence = lease_fence + 1,
        lease_expires_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', printf('+%d seconds', ?2)), attempt = attempt + 1,
        started_at = COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE tenant_id = ?3 AND id = ?4")
        .bind(&params.worker_id).bind(seconds).bind(&params.tenant_id).bind(&id)
        .execute(&mut *tx).await.map_err(read::error)?.rows_affected();
    read::changed(affected)?;
    if before.status == ProductJobStatus::Running {
        sqlx::query("UPDATE product_job_node SET status = 'pending', result = NULL, error = NULL,
            started_at = NULL, finished_at = NULL WHERE tenant_id = ?1 AND job_id = ?2 AND status <> 'succeeded'")
            .bind(&params.tenant_id).bind(&id).execute(&mut *tx).await.map_err(read::error)?;
    }
    let job = read::required(&mut tx, &params.tenant_id, &id).await?;
    tx.commit().await.map_err(read::error)?;
    Ok(Some(job))
}

pub(super) async fn heartbeat(
    repo: &SqliteProductJobRepo,
    params: &HeartbeatProductJobParams,
) -> Result<ProductJob, BackendError> {
    let seconds = validate::lease_seconds(params.lease_seconds)?;
    let mut tx = repo.begin_write().await?;
    let affected = sqlx::query("UPDATE product_job SET
        lease_expires_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', printf('+%d seconds', ?1)),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' AND lease_owner = ?4 AND lease_fence = ?5
        AND julianday(lease_expires_at) > julianday('now')")
        .bind(seconds).bind(&params.tenant_id).bind(&params.job_id)
        .bind(&params.lease.worker_id).bind(params.lease.fence)
        .execute(&mut *tx).await.map_err(read::error)?.rows_affected();
    read::changed(affected)?;
    let job = read::required(&mut tx, &params.tenant_id, &params.job_id).await?;
    tx.commit().await.map_err(read::error)?;
    Ok(job)
}
