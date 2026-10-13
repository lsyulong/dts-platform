use super::{SqliteServiceAccessRepo, authority, storage::*};
use dts_backend::{BackendError, repo::service_access::*};
use sqlx::Row;

pub(super) async fn list(
    repo: &SqliteServiceAccessRepo,
    f: &ServiceCallFilter,
) -> Result<ServiceCallPage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    let authority = authority::active(&mut tx, &f.tenant_id, &f.actor_id).await?;
    let total:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM service_call WHERE tenant_id=?1 AND (owner_id=?2 OR ?3=1) AND (?4 IS NULL OR application_id=?4) AND (?5 IS NULL OR api_id=?5)) + (SELECT COUNT(*) FROM service_call_denial WHERE tenant_id=?1 AND (owner_id=?2 OR ?3=1) AND (?4 IS NULL OR application_id=?4) AND (?5 IS NULL OR api_id=?5))")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(authority.can_approve).bind(&f.application_id).bind(&f.api_id).fetch_one(&mut *tx).await.map_err(error)?;
    let rows=sqlx::query("SELECT * FROM (SELECT id,tenant_id,owner_id,application_id,credential_id,api_id,NULL AS target_hash,outcome,NULL AS reason_code,started_at,finished_at,duration_ms,row_count FROM service_call WHERE tenant_id=?1 AND (owner_id=?2 OR ?3=1) AND (?4 IS NULL OR application_id=?4) AND (?5 IS NULL OR api_id=?5) UNION ALL SELECT id,tenant_id,owner_id,application_id,credential_id,api_id,target_hash,'denied' AS outcome,reason_code,created_at AS started_at,created_at AS finished_at,NULL AS duration_ms,NULL AS row_count FROM service_call_denial WHERE tenant_id=?1 AND (owner_id=?2 OR ?3=1) AND (?4 IS NULL OR application_id=?4) AND (?5 IS NULL OR api_id=?5)) ORDER BY started_at DESC,id LIMIT ?6 OFFSET ?7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(authority.can_approve).bind(&f.application_id).bind(&f.api_id).bind(f.limit as i64).bind(f.offset as i64)
        .fetch_all(&mut *tx).await.map_err(error)?;
    let mut data = Vec::new();
    for row in rows {
        let api: Option<String> = row.try_get("api_id").map_err(error)?;
        let outcome = match row.try_get::<String, _>("outcome").map_err(error)?.as_str() {
            "pending" => ServiceCallOutcome::Pending,
            "succeeded" => ServiceCallOutcome::Succeeded,
            "failed" => ServiceCallOutcome::Failed,
            "denied" => ServiceCallOutcome::Denied,
            "cancelled" => ServiceCallOutcome::Cancelled,
            "reconciliation_required" => ServiceCallOutcome::ReconciliationRequired,
            _ => return Err(invalid("真实调用审计状态无效")),
        };
        data.push(ServiceCallAudit {
            id: row.try_get("id").map_err(error)?,
            tenant_id: row.try_get("tenant_id").map_err(error)?,
            owner_id: row.try_get("owner_id").map_err(error)?,
            application_id: row.try_get("application_id").map_err(error)?,
            credential_id: row.try_get("credential_id").map_err(error)?,
            target_hash: row
                .try_get::<Option<String>, _>("target_hash")
                .map_err(error)?
                .unwrap_or_else(|| validation::target_hash(api.as_deref().unwrap_or(""))),
            api_id: api,
            outcome,
            reason_code: row.try_get("reason_code").map_err(error)?,
            started_at: time(&row, "started_at")?,
            finished_at: optional_time(&row, "finished_at")?,
            duration_ms: row
                .try_get::<Option<i64>, _>("duration_ms")
                .map_err(error)?
                .map(|value| value as u64),
            row_count: row
                .try_get::<Option<i64>, _>("row_count")
                .map_err(error)?
                .map(|value| value as u64),
        });
    }
    tx.commit().await.map_err(error)?;
    Ok(ServiceCallPage { data, total })
}
