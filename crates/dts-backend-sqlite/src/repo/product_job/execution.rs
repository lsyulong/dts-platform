// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use dts_backend::{BackendError, repo::product_job::validation as validate, repo::product_job::*};

use super::{SqliteProductJobRepo, read};

fn node(job: &ProductJob, id: &str) -> Result<ProductJobNode, BackendError> {
    job.nodes
        .iter()
        .find(|node| node.id == id)
        .cloned()
        .ok_or_else(|| BackendError::NotFound("运行节点不存在".into()))
}

pub(super) async fn start_node(
    repo: &SqliteProductJobRepo,
    params: &StartProductJobNodeParams,
) -> Result<ProductJobNode, BackendError> {
    let mut tx = repo.begin_write().await?;
    let job = read::leased(&mut tx, &params.tenant_id, &params.job_id, &params.lease).await?;
    let before = node(&job, &params.node_id)?;
    if job.cancel_requested || before.status != ProductJobNodeStatus::Pending {
        return Err(BackendError::Conflict(
            "节点不可执行或运行已请求取消".into(),
        ));
    }
    if before.attempt == i64::MAX
        || before.depends_on.iter().any(|dependency| {
            !job.nodes.iter().any(|node| {
                node.id == *dependency && node.status == ProductJobNodeStatus::Succeeded
            })
        })
    {
        return Err(BackendError::Conflict(
            "前置节点未全部成功或节点尝试计数已达上限".into(),
        ));
    }
    let affected = sqlx::query("UPDATE product_job_node SET status = 'running', attempt = attempt + 1,
        started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), finished_at = NULL, result = NULL, error = NULL
        WHERE tenant_id = ?1 AND job_id = ?2 AND node_id = ?3 AND status = 'pending'
        AND EXISTS (SELECT id FROM product_job WHERE tenant_id = ?1 AND id = ?2 AND status = 'running'
            AND lease_owner = ?4 AND lease_fence = ?5 AND cancel_requested = 0
            AND julianday(lease_expires_at) > julianday('now'))")
        .bind(&params.tenant_id).bind(&params.job_id).bind(&params.node_id)
        .bind(&params.lease.worker_id).bind(params.lease.fence)
        .execute(&mut *tx).await.map_err(read::error)?.rows_affected();
    read::changed(affected)?;
    let saved = node(
        &read::required(&mut tx, &params.tenant_id, &params.job_id).await?,
        &params.node_id,
    )?;
    tx.commit().await.map_err(read::error)?;
    Ok(saved)
}

pub(super) async fn finish_node(
    repo: &SqliteProductJobRepo,
    params: &FinishProductJobNodeParams,
) -> Result<ProductJobNode, BackendError> {
    let (result, error) = validate::completion(params.outcome, &params.result, &params.error)?;
    let mut tx = repo.begin_write().await?;
    let job = read::leased(&mut tx, &params.tenant_id, &params.job_id, &params.lease).await?;
    validate::check_node_result_budget(&job, &params.node_id, &params.result)?;
    if node(&job, &params.node_id)?.status != ProductJobNodeStatus::Running {
        return Err(BackendError::Conflict("节点未执行或已经结束".into()));
    }
    let affected = sqlx::query("UPDATE product_job_node SET status = ?1, result = ?2, error = ?3,
        finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE tenant_id = ?4 AND job_id = ?5 AND node_id = ?6 AND status = 'running'
        AND EXISTS (SELECT id FROM product_job WHERE tenant_id = ?4 AND id = ?5 AND status = 'running'
            AND lease_owner = ?7 AND lease_fence = ?8 AND julianday(lease_expires_at) > julianday('now'))")
        .bind(validate::outcome_text(params.outcome)).bind(result).bind(error)
        .bind(&params.tenant_id).bind(&params.job_id).bind(&params.node_id)
        .bind(&params.lease.worker_id).bind(params.lease.fence)
        .execute(&mut *tx).await.map_err(read::error)?.rows_affected();
    read::changed(affected)?;
    if matches!(params.outcome, ProductJobOutcome::Failed) {
        skip_descendants(&mut tx, &job, &params.node_id).await?;
    }
    let saved = node(
        &read::required(&mut tx, &params.tenant_id, &params.job_id).await?,
        &params.node_id,
    )?;
    tx.commit().await.map_err(read::error)?;
    Ok(saved)
}

async fn skip_descendants(
    conn: &mut sqlx::SqliteConnection,
    job: &ProductJob,
    failed: &str,
) -> Result<(), BackendError> {
    let mut blocked = HashSet::from([failed.to_owned()]);
    loop {
        let before = blocked.len();
        for node in &job.nodes {
            if node
                .depends_on
                .iter()
                .any(|dependency| blocked.contains(dependency))
            {
                blocked.insert(node.id.clone());
            }
        }
        if blocked.len() == before {
            break;
        }
    }
    for id in blocked {
        if id == failed {
            continue;
        }
        sqlx::query(
            "UPDATE product_job_node SET status = 'skipped', error = '依赖节点失败',
            finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE tenant_id = ?1 AND job_id = ?2 AND node_id = ?3 AND status = 'pending'",
        )
        .bind(&job.tenant_id)
        .bind(&job.id)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(read::error)?;
    }
    Ok(())
}

pub(super) async fn finish(
    repo: &SqliteProductJobRepo,
    params: &FinishProductJobParams,
) -> Result<ProductJob, BackendError> {
    let (result, error) = validate::completion(params.outcome, &params.result, &params.error)?;
    let mut tx = repo.begin_write().await?;
    let before = read::leased(&mut tx, &params.tenant_id, &params.job_id, &params.lease).await?;
    validate::check_job_completion(&before, params.outcome)?;
    let status = validate::outcome_text(params.outcome);
    let affected = sqlx::query("UPDATE product_job SET status = ?1, result = ?2, error = ?3,
        lease_owner = NULL, lease_expires_at = NULL, finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE tenant_id = ?4 AND id = ?5 AND status = 'running'
        AND lease_owner = ?6 AND lease_fence = ?7 AND julianday(lease_expires_at) > julianday('now')")
        .bind(status).bind(result).bind(error).bind(&params.tenant_id).bind(&params.job_id)
        .bind(&params.lease.worker_id).bind(params.lease.fence)
        .execute(&mut *tx).await.map_err(read::error)?.rows_affected();
    read::changed(affected)?;
    if status != "succeeded" {
        let pending_status = if status == "failed" {
            "skipped"
        } else {
            "cancelled"
        };
        sqlx::query("UPDATE product_job_node SET status = ?1, finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            error = CASE WHEN ?1 = 'skipped' THEN '运行失败后未执行' ELSE NULL END
            WHERE tenant_id = ?2 AND job_id = ?3 AND status = 'pending'")
            .bind(pending_status).bind(&params.tenant_id).bind(&params.job_id)
            .execute(&mut *tx).await.map_err(read::error)?;
    }
    let saved = read::required(&mut tx, &params.tenant_id, &params.job_id).await?;
    tx.commit().await.map_err(read::error)?;
    Ok(saved)
}
