use super::{
    SqliteServiceAccessRepo, application, authority, credentials, storage::*, subscription,
};
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::service_access::*};
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

pub(super) fn decode(row: &SqliteRow) -> Result<ServiceCallRecord, BackendError> {
    let outcome = match row.try_get::<String, _>("outcome").map_err(error)?.as_str() {
        "pending" => ServiceCallOutcome::Pending,
        "succeeded" => ServiceCallOutcome::Succeeded,
        "failed" => ServiceCallOutcome::Failed,
        "denied" => ServiceCallOutcome::Denied,
        "cancelled" => ServiceCallOutcome::Cancelled,
        "reconciliation_required" => ServiceCallOutcome::ReconciliationRequired,
        _ => return Err(invalid("真实调用结果状态无效")),
    };
    Ok(ServiceCallRecord {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        owner_id: row.try_get("owner_id").map_err(error)?,
        application_id: row.try_get("application_id").map_err(error)?,
        application_version: row.try_get("application_version").map_err(error)?,
        credential_id: row.try_get("credential_id").map_err(error)?,
        credential_version: row.try_get("credential_version").map_err(error)?,
        subscription_id: row.try_get("subscription_id").map_err(error)?,
        subscription_version: row.try_get("subscription_version").map_err(error)?,
        api_id: row.try_get("api_id").map_err(error)?,
        api_version: row.try_get("api_version").map_err(error)?,
        minute_bucket: time(row, "minute_bucket")?,
        rate_limit: row
            .try_get::<i64, _>("rate_limit")
            .map_err(error)?
            .try_into()
            .map_err(|_| invalid("真实限额无效"))?,
        origin: row.try_get("origin").map_err(error)?,
        outcome,
        started_at: time(row, "started_at")?,
        finished_at: optional_time(row, "finished_at")?,
        duration_ms: row
            .try_get::<Option<i64>, _>("duration_ms")
            .map_err(error)?
            .map(|value| value as u64),
        row_count: row
            .try_get::<Option<i64>, _>("row_count")
            .map_err(error)?
            .map(|value| value as u64),
    })
}
async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<ServiceCallRecord, BackendError> {
    let row = sqlx::query("SELECT * FROM service_call WHERE tenant_id=?1 AND id=?2")
        .bind(tenant)
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .ok_or_else(|| invalid("服务调用真实记录不存在"))?;
    decode(&row)
}
fn identity(expected: &ServiceCallRecord, actual: &ServiceCallRecord) -> bool {
    let mut expected = expected.clone();
    let mut actual = actual.clone();
    expected.outcome = ServiceCallOutcome::Pending;
    actual.outcome = ServiceCallOutcome::Pending;
    expected.finished_at = None;
    actual.finished_at = None;
    expected.duration_ms = None;
    actual.duration_ms = None;
    expected.row_count = None;
    actual.row_count = None;
    expected == actual
}
pub(super) async fn check(
    conn: &mut SqliteConnection,
    expected: &ServiceCallRecord,
) -> Result<(), BackendError> {
    let actual = get(conn, &expected.tenant_id, &expected.id).await?;
    if !identity(expected, &actual) || actual.outcome != ServiceCallOutcome::Pending {
        return Err(conflict("真实调用身份、许可版本或状态已改变"));
    }
    let credential = credentials::lookup(conn, &actual.credential_id)
        .await?
        .ok_or_else(|| invalid("调用凭据已不存在"))?;
    let app = application::get(conn, &actual.tenant_id, &actual.application_id)
        .await?
        .ok_or_else(|| invalid("调用应用已不存在"))?;
    let sub = subscription::get(conn, &actual.tenant_id, &actual.subscription_id)
        .await?
        .ok_or_else(|| invalid("调用订阅已不存在"))?;
    let now = now(conn).await?;
    authority::active(conn, &actual.tenant_id, &actual.owner_id).await?;
    if credential.summary.tenant_id != actual.tenant_id
        || credential.summary.application_id != actual.application_id
        || credential.summary.owner_id != actual.owner_id
        || credential.summary.version != actual.credential_version
        || credential.summary.revoked_at.is_some()
        || credential.summary.expires_at <= now
        || app.resource.status != "active"
        || app.resource.version != actual.application_version
        || app.owner_id != actual.owner_id
        || app.credential_expires_at <= now
        || sub.resource.status != "approved"
        || sub.resource.version != actual.subscription_version
        || sub.owner_id != actual.owner_id
        || sub.application_id != actual.application_id
        || sub.api_id != actual.api_id
        || sub.api_version != actual.api_version
        || sub.expires_at <= now
        || !origin_allowed(&app.allowed_origins, &actual.origin)
    {
        return Err(conflict("调用期间凭据、应用、订阅或来源许可已失效"));
    }
    subscription::api(conn, &actual.tenant_id, &actual.api_id, actual.api_version).await?;
    Ok(())
}
fn origin_allowed(origins: &[String], origin: &Option<String>) -> bool {
    match origin {
        Some(value) => origins.contains(value),
        None => origins.is_empty(),
    }
}
pub(super) async fn reserve(
    repo: &SqliteServiceAccessRepo,
    p: &ReserveServiceCallParams,
) -> Result<ServiceCallRecord, BackendError> {
    validation::reserve(p)?;
    let mut tx = repo.begin().await?;
    let credential = credentials::lookup(&mut tx, &p.credential_id)
        .await?
        .filter(|value| {
            value.summary.tenant_id == p.tenant_id && value.verifier == p.credential_verifier
        })
        .ok_or_else(|| invalid("服务凭据身份无法核验"))?;
    let api_id = resource(&mut tx, &p.tenant_id, "data_api", &p.api_id)
        .await?
        .map(|api| api.id);
    let mut reason = None;
    let now = now(&mut tx).await?;
    let app =
        match application::get(&mut tx, &p.tenant_id, &credential.summary.application_id).await {
            Ok(Some(app)) => app,
            Ok(None)
            | Err(BackendError::Conflict(_))
            | Err(BackendError::ConstraintViolation(_))
            | Err(BackendError::NotFound(_)) => {
                let error = deny(
                    &mut tx,
                    p,
                    &credential.summary,
                    api_id,
                    "application_inactive",
                    now,
                )
                .await?;
                tx.commit().await.map_err(super::storage::error)?;
                return Err(error);
            },
            Err(error) => return Err(error),
        };
    let owner = authority::get(&mut tx, &p.tenant_id, &credential.summary.owner_id).await?;
    if credential.summary.version != p.credential_version
        || credential.summary.revoked_at.is_some()
        || credential.summary.expires_at <= now
    {
        reason = Some("credential_unavailable");
    } else if !owner.active {
        reason = Some("owner_inactive");
    } else if app.resource.status != "active"
        || app.owner_id != credential.summary.owner_id
        || app.credential_expires_at <= now
    {
        reason = Some("application_inactive");
    } else if !origin_allowed(&app.allowed_origins, &p.origin) {
        reason = Some("origin_denied");
    }
    let sub_id:Option<String>=sqlx::query_scalar("SELECT subscription_id FROM service_subscription WHERE tenant_id=?1 AND application_id=?2 AND api_id=?3 AND status='approved'")
        .bind(&p.tenant_id).bind(&app.resource.id).bind(&p.api_id).fetch_optional(&mut *tx).await.map_err(error)?;
    let sub = match sub_id {
        Some(id) => match subscription::get(&mut tx, &p.tenant_id, &id).await {
            Ok(sub) => sub,
            Err(BackendError::Conflict(_))
            | Err(BackendError::ConstraintViolation(_))
            | Err(BackendError::NotFound(_)) => {
                if reason.is_none() {
                    reason = Some("subscription_unavailable");
                }
                None
            },
            Err(error) => return Err(error),
        },
        None => None,
    };
    if reason.is_none()
        && sub
            .as_ref()
            .is_none_or(|sub| sub.expires_at <= now || sub.owner_id != app.owner_id)
    {
        reason = Some("subscription_unavailable");
    }
    let api = if let Some(sub) = &sub {
        match subscription::api(&mut tx, &p.tenant_id, &p.api_id, sub.api_version).await {
            Ok(api) => Some(api),
            Err(BackendError::Conflict(_))
            | Err(BackendError::ConstraintViolation(_))
            | Err(BackendError::NotFound(_)) => {
                if reason.is_none() {
                    reason = Some("api_unavailable");
                }
                None
            },
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let rate = api
        .as_ref()
        .and_then(|api| api.properties["rate_limit"].as_u64())
        .filter(|rate| (1..=100000).contains(rate))
        .map(|rate| rate as u32);
    if reason.is_none() && rate.is_none() {
        reason = Some("api_unavailable");
    }
    let minute = DateTime::<Utc>::from_timestamp(now.timestamp().div_euclid(60) * 60, 0)
        .ok_or_else(|| invalid("当前分钟窗口无效"))?;
    let api_count:Option<i64>=sqlx::query_scalar("SELECT reserved_count FROM service_api_minute_quota WHERE tenant_id=?1 AND api_id=?2 AND minute_bucket=?3")
        .bind(&p.tenant_id).bind(&p.api_id).bind(minute.to_rfc3339()).fetch_optional(&mut *tx).await.map_err(error)?;
    let credential_count:Option<i64>=sqlx::query_scalar("SELECT reserved_count FROM service_credential_minute_quota WHERE tenant_id=?1 AND credential_id=?2 AND api_id=?3 AND minute_bucket=?4")
        .bind(&p.tenant_id).bind(&p.credential_id).bind(&p.api_id).bind(minute.to_rfc3339()).fetch_optional(&mut *tx).await.map_err(error)?;
    if reason.is_none()
        && rate.is_some_and(|rate| {
            api_count.unwrap_or(0) >= rate as i64 || credential_count.unwrap_or(0) >= rate as i64
        })
    {
        reason = Some("rate_limited");
    }
    if let Some(reason) = reason {
        let denied = deny(&mut tx, p, &credential.summary, api_id, reason, now).await?;
        tx.commit().await.map_err(error)?;
        return Err(denied);
    }
    let sub = sub.ok_or_else(|| invalid("真实许可不存在"))?;
    let rate = rate.ok_or_else(|| invalid("真实限额不存在"))?;
    sqlx::query("INSERT INTO service_api_minute_quota(tenant_id,api_id,minute_bucket,reserved_count) VALUES(?1,?2,?3,1) ON CONFLICT(tenant_id,api_id,minute_bucket) DO UPDATE SET reserved_count=reserved_count+1")
        .bind(&p.tenant_id).bind(&p.api_id).bind(minute.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
    sqlx::query("INSERT INTO service_credential_minute_quota(tenant_id,credential_id,api_id,minute_bucket,reserved_count) VALUES(?1,?2,?3,?4,1) ON CONFLICT(tenant_id,credential_id,api_id,minute_bucket) DO UPDATE SET reserved_count=reserved_count+1")
        .bind(&p.tenant_id).bind(&p.credential_id).bind(&p.api_id).bind(minute.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
    sqlx::query("INSERT INTO service_call(id,tenant_id,owner_id,application_id,application_version,credential_id,credential_version,subscription_id,subscription_version,api_id,api_version,minute_bucket,rate_limit,origin,outcome,started_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'pending',?15)")
        .bind(&p.call_id).bind(&p.tenant_id).bind(&app.owner_id).bind(&app.resource.id).bind(app.resource.version).bind(&p.credential_id).bind(p.credential_version)
        .bind(&sub.resource.id).bind(sub.resource.version).bind(&p.api_id).bind(sub.api_version).bind(minute.to_rfc3339()).bind(rate as i64).bind(&p.origin).bind(now.to_rfc3339())
        .execute(&mut *tx).await.map_err(error)?;
    let record = get(&mut tx, &p.tenant_id, &p.call_id).await?;
    tx.commit().await.map_err(error)?;
    Ok(record)
}
async fn deny(
    conn: &mut SqliteConnection,
    p: &ReserveServiceCallParams,
    credential: &ServiceCredentialSummary,
    api_id: Option<String>,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<BackendError, BackendError> {
    sqlx::query("INSERT INTO service_call_denial(id,tenant_id,owner_id,application_id,credential_id,api_id,target_hash,reason_code,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(&p.call_id).bind(&p.tenant_id).bind(&credential.owner_id).bind(&credential.application_id).bind(&p.credential_id).bind(api_id)
        .bind(validation::target_hash(&p.api_id)).bind(reason).bind(now.to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(if reason == "rate_limited" {
        BackendError::RateLimited {
            retry_after_secs: (60 - now.timestamp().rem_euclid(60)) as u32,
        }
    } else {
        BackendError::Forbidden("应用调用未获得当前有效许可".into())
    })
}
pub(super) async fn finish(
    repo: &SqliteServiceAccessRepo,
    p: &FinishServiceCallParams,
) -> Result<ServiceCallRecord, BackendError> {
    validation::finish(p)?;
    let mut tx = repo.begin().await?;
    let current = get(&mut tx, &p.call.tenant_id, &p.call.id).await?;
    if !identity(&p.call, &current) {
        return Err(conflict("调用结果不属于原始真实许可"));
    }
    if current.outcome != ServiceCallOutcome::Pending {
        if current.outcome == p.outcome && current.row_count == p.row_count {
            super::access_audit::replay(&mut tx, p).await?;
            tx.commit().await.map_err(error)?;
            return Ok(current);
        }
        return Err(conflict("已经完成的调用不能改写结果"));
    }
    if p.outcome == ServiceCallOutcome::Succeeded {
        check(&mut tx, &p.call).await?;
    }
    super::access_audit::write(&mut tx, p).await?;
    let now = now(&mut tx).await?;
    sqlx::query("UPDATE service_call SET outcome=?1,finished_at=?2,duration_ms=?3,row_count=?4 WHERE tenant_id=?5 AND id=?6 AND outcome='pending'")
        .bind(p.outcome.as_str()).bind(now.to_rfc3339()).bind(p.duration_ms as i64).bind(p.row_count.map(|value|value as i64))
        .bind(&p.call.tenant_id).bind(&p.call.id).execute(&mut *tx).await.map_err(error)?;
    let value = get(&mut tx, &p.call.tenant_id, &p.call.id).await?;
    tx.commit().await.map_err(error)?;
    Ok(value)
}
