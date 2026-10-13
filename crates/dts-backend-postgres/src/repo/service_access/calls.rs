use dts_backend::{
    BackendError,
    repo::service_access::{validation, *},
};
use sqlx::{PgConnection, Row, postgres::PgRow};

use super::{
    application, authority, call_policy, credentials, quota,
    storage::{self, conflict, error, invalid},
    subscription,
};

pub(super) enum ReservationOutcome {
    Allowed(Box<ServiceCallRecord>),
    Denied(BackendError),
}

pub(super) fn outcome(value: &str) -> Result<ServiceCallOutcome, BackendError> {
    match value {
        "pending" => Ok(ServiceCallOutcome::Pending),
        "succeeded" => Ok(ServiceCallOutcome::Succeeded),
        "failed" => Ok(ServiceCallOutcome::Failed),
        "denied" => Ok(ServiceCallOutcome::Denied),
        "cancelled" => Ok(ServiceCallOutcome::Cancelled),
        "reconciliation_required" => Ok(ServiceCallOutcome::ReconciliationRequired),
        _ => Err(invalid("持久化调用结果状态无效")),
    }
}
pub(super) fn unsigned(value: Option<i64>) -> Result<Option<u64>, BackendError> {
    value
        .map(|v| v.try_into().map_err(|_| invalid("调用耗时或行数无效")))
        .transpose()
}
fn decode(row: &PgRow) -> Result<ServiceCallRecord, BackendError> {
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
        minute_bucket: row.try_get("minute_bucket").map_err(error)?,
        rate_limit: row
            .try_get::<i64, _>("rate_limit")
            .map_err(error)?
            .try_into()
            .map_err(|_| invalid("调用限额无效"))?,
        origin: row.try_get("origin").map_err(error)?,
        outcome: outcome(&row.try_get::<String, _>("outcome").map_err(error)?)?,
        started_at: row.try_get("started_at").map_err(error)?,
        finished_at: row.try_get("finished_at").map_err(error)?,
        duration_ms: unsigned(row.try_get("duration_ms").map_err(error)?)?,
        row_count: unsigned(row.try_get("row_count").map_err(error)?)?,
    })
}
async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<ServiceCallRecord, BackendError> {
    sqlx::query("SELECT * FROM service_call WHERE tenant_id=$1 AND id=$2 FOR UPDATE")
        .bind(tenant)
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode)
        .transpose()?
        .ok_or_else(|| invalid("真实调用许可记录不存在"))
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

fn denial_error(reason: &str, now: chrono::DateTime<chrono::Utc>) -> BackendError {
    if reason == "rate_limited" {
        BackendError::RateLimited {
            retry_after_secs: quota::retry_after_secs(now),
        }
    } else {
        BackendError::Forbidden("应用调用未获得当前有效许可".into())
    }
}

pub(super) async fn reserve(
    conn: &mut PgConnection,
    p: &ReserveServiceCallParams,
) -> Result<ReservationOutcome, BackendError> {
    validation::reserve(p)?;
    let credential = credentials::lookup(conn, &p.credential_id)
        .await?
        .filter(|c| c.summary.tenant_id == p.tenant_id && c.verifier == p.credential_verifier)
        .ok_or_else(|| invalid("服务调用凭据无法核验"))?;
    let mut context = call_policy::check(conn, p, &credential).await?;
    let actual = credentials::locked(conn, &p.tenant_id, &p.credential_id).await?;
    if actual.verifier != p.credential_verifier {
        return Err(conflict("凭据身份在验证期间发生非法改变"));
    }
    if actual.summary.version != p.credential_version || actual.summary.revoked_at.is_some() {
        context.reason = Some("credential_unavailable");
    }
    if context.reason.is_none() {
        let rate = context
            .rate_limit
            .ok_or_else(|| invalid("实际API限额缺失"))?;
        if !quota::reserve(conn, p, context.minute, rate).await? {
            context.reason = Some("rate_limited");
        }
    }
    if let Some(reason) = context.reason {
        denial(conn, p, &actual, reason).await?;
        return Ok(ReservationOutcome::Denied(denial_error(
            reason,
            context.now,
        )));
    }
    let app = context
        .application
        .ok_or_else(|| invalid("实际应用身份不存在"))?;
    let sub = context
        .subscription
        .ok_or_else(|| invalid("实际审批许可不存在"))?;
    let rate = context
        .rate_limit
        .ok_or_else(|| invalid("实际API限额缺失"))?;
    sqlx::query("INSERT INTO service_call(id,tenant_id,owner_id,application_id,application_version,credential_id,credential_version,
        subscription_id,subscription_version,api_id,api_version,minute_bucket,rate_limit,origin,outcome,started_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'pending',$15)")
        .bind(&p.call_id).bind(&p.tenant_id).bind(&actual.summary.owner_id).bind(&app.resource.id).bind(app.resource.version)
        .bind(&p.credential_id).bind(p.credential_version).bind(&sub.resource.id).bind(sub.resource.version).bind(&p.api_id).bind(sub.api_version)
        .bind(context.minute).bind(i64::from(rate)).bind(&p.origin).bind(context.now).execute(&mut *conn).await.map_err(error)?;
    Ok(ReservationOutcome::Allowed(Box::new(
        get(conn, &p.tenant_id, &p.call_id).await?,
    )))
}

async fn denial(
    conn: &mut PgConnection,
    p: &ReserveServiceCallParams,
    credential: &ServiceCredentialRecord,
    reason: &str,
) -> Result<(), BackendError> {
    let api_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM product_resource WHERE tenant_id=$1 AND kind='data_api' AND id=$2",
    )
    .bind(&p.tenant_id)
    .bind(&p.api_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(error)?;
    let now = storage::now(conn).await?;
    sqlx::query("INSERT INTO service_call_denial(id,tenant_id,owner_id,application_id,credential_id,api_id,target_hash,reason_code,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&p.call_id).bind(&p.tenant_id).bind(&credential.summary.owner_id).bind(&credential.summary.application_id).bind(&p.credential_id).bind(api_id)
        .bind(validation::target_hash(&p.api_id)).bind(reason).bind(now).execute(conn).await.map_err(error)?;
    Ok(())
}

pub(super) async fn reauthorize(
    conn: &mut PgConnection,
    expected: &ServiceCallRecord,
) -> Result<(), BackendError> {
    let actual = get(conn, &expected.tenant_id, &expected.id).await?;
    if !identity(expected, &actual) || actual.outcome != ServiceCallOutcome::Pending {
        return Err(conflict("调用身份、许可版本或结果状态已变化"));
    }
    let owner = authority::get(conn, &actual.tenant_id, &actual.owner_id, true).await?;
    if !owner.active {
        return Err(conflict("调用所有者或租户已停用"));
    }
    let app = application::required(conn, &actual.tenant_id, &actual.application_id, true).await?;
    let credential = credentials::locked(conn, &actual.tenant_id, &actual.credential_id).await?;
    let sub =
        subscription::required(conn, &actual.tenant_id, &actual.subscription_id, true).await?;
    let now = storage::now(conn).await?;
    if credential.summary.application_id != actual.application_id
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
        || !call_policy::origin_allowed(&app.allowed_origins, &actual.origin)
    {
        return Err(conflict("调用期间凭据、应用、审批许可或来源已失效"));
    }
    subscription::api(
        conn,
        &actual.tenant_id,
        &actual.api_id,
        actual.api_version,
        true,
    )
    .await?;
    Ok(())
}

pub(super) async fn finish(
    conn: &mut PgConnection,
    p: &FinishServiceCallParams,
) -> Result<ServiceCallRecord, BackendError> {
    validation::finish(p)?;
    let current = get(conn, &p.call.tenant_id, &p.call.id).await?;
    if !identity(&p.call, &current) {
        return Err(conflict("结果不属于实际原始调用许可"));
    }
    if current.outcome != ServiceCallOutcome::Pending {
        if current.outcome == p.outcome && current.row_count == p.row_count {
            super::access_audit::replay(conn, p).await?;
            return Ok(current);
        }
        return Err(conflict("已完成调用不能改写结果"));
    }
    if p.outcome == ServiceCallOutcome::Succeeded {
        reauthorize(conn, &p.call).await?;
    }
    super::access_audit::write(conn, p).await?;
    let now = storage::now(conn).await?;
    let duration: i64 = p
        .duration_ms
        .try_into()
        .map_err(|_| invalid("调用耗时溢出"))?;
    let rows: Option<i64> = p
        .row_count
        .map(|r| r.try_into().map_err(|_| invalid("调用行数溢出")))
        .transpose()?;
    let changed=sqlx::query("UPDATE service_call SET outcome=$1,finished_at=$2,duration_ms=$3,row_count=$4 WHERE tenant_id=$5 AND id=$6 AND outcome='pending'")
        .bind(p.outcome.as_str()).bind(now).bind(duration).bind(rows).bind(&p.call.tenant_id).bind(&p.call.id).execute(&mut *conn).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("调用结果发生并发写入"));
    }
    get(conn, &p.call.tenant_id, &p.call.id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_policy_denials_are_forbidden_and_do_not_expose_source_details() {
        let now = chrono::DateTime::parse_from_rfc3339("2027-03-04T05:06:59.999999Z")
            .expect("time")
            .with_timezone(&chrono::Utc);
        for reason in [
            "credential_unavailable",
            "owner_inactive",
            "application_inactive",
            "origin_denied",
            "api_unavailable",
            "subscription_unavailable",
        ] {
            assert!(
                matches!(denial_error(reason,now),BackendError::Forbidden(message) if message=="应用调用未获得当前有效许可")
            );
        }
        assert!(matches!(
            denial_error("rate_limited", now),
            BackendError::RateLimited {
                retry_after_secs: 1
            }
        ));
    }
    #[test]
    fn terminal_state_does_not_change_identity_but_frozen_permission_versions_do() {
        let now = chrono::Utc::now();
        let original = ServiceCallRecord {
            id: "call".into(),
            tenant_id: "tenant".into(),
            owner_id: "owner".into(),
            application_id: "app".into(),
            application_version: 1,
            credential_id: "credential".into(),
            credential_version: 1,
            subscription_id: "subscription".into(),
            subscription_version: 3,
            api_id: "api".into(),
            api_version: 2,
            minute_bucket: now,
            rate_limit: 10,
            origin: None,
            outcome: ServiceCallOutcome::Pending,
            started_at: now,
            finished_at: None,
            duration_ms: None,
            row_count: None,
        };
        let mut terminal = original.clone();
        terminal.outcome = ServiceCallOutcome::Succeeded;
        terminal.finished_at = Some(now);
        terminal.duration_ms = Some(50);
        terminal.row_count = Some(0);
        assert!(identity(&original, &terminal));
        terminal.subscription_version += 1;
        assert!(!identity(&original, &terminal));
        terminal = original.clone();
        terminal.credential_id = "other".into();
        assert!(!identity(&original, &terminal));
    }
    #[test]
    fn invalid_negative_observed_numbers_are_rejected() {
        assert_eq!(unsigned(Some(0)).expect("zero"), Some(0));
        assert_eq!(unsigned(None).expect("none"), None);
        assert!(unsigned(Some(-1)).is_err());
    }
}
