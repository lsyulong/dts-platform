use dts_backend::{
    BackendError,
    repo::service_access::{validation, *},
};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

use super::{
    application, authority,
    storage::{self, WriteScope, conflict, error, invalid},
    subscription,
};

pub(super) async fn decide(
    conn: &mut PgConnection,
    p: &DecideServiceSubscriptionParams,
) -> Result<ServiceSubscriptionDecisionResult, BackendError> {
    let semantic = validation::decision_semantics(p)?;
    let actor = authority::get(conn, &p.tenant_id, &p.actor_id, true).await?;
    if !actor.active || (p.decision.needs_approver() && !actor.can_approve) {
        return Err(invalid("用户失效或缺少实际services:approve"));
    }
    let before = subscription::required(conn, &p.tenant_id, &p.subscription_id, true).await?;
    if !p.decision.needs_approver() {
        authority::manage(&actor, &p.actor_id, &before.owner_id)?;
    }
    if let Some(result) = receipt(conn, p, &semantic).await? {
        return Ok(result);
    }
    storage::version(&before.resource, p.expected_version)?;
    if p.decision.needs_approver() {
        authority::approve(&actor, &p.actor_id, &before.owner_id, &before.requester_id)?;
    }
    let next = match (p.decision, before.resource.status.as_str()) {
        (ServiceSubscriptionDecision::Submit, "draft") => "pending",
        (ServiceSubscriptionDecision::Approve, "pending") => "approved",
        (ServiceSubscriptionDecision::Reject, "pending") => "rejected",
        (ServiceSubscriptionDecision::Revoke, "approved") => "revoked",
        _ => return Err(conflict("当前订阅状态不允许此审批动作")),
    };
    let now = storage::now(conn).await?;
    let expires = storage::canonical_time(p.expires_at.unwrap_or(before.expires_at))?;
    if matches!(
        p.decision,
        ServiceSubscriptionDecision::Submit | ServiceSubscriptionDecision::Approve
    ) {
        let app = application::required(conn, &p.tenant_id, &before.application_id, true).await?;
        authority::active(conn, &p.tenant_id, &app.owner_id).await?;
        if app.resource.status != "active" || app.owner_id != before.owner_id {
            return Err(conflict("订阅应用已停用或所有者变化"));
        }
        subscription::api(conn, &p.tenant_id, &before.api_id, before.api_version, true).await?;
        if expires <= now || expires > before.expires_at {
            return Err(invalid("批准有效期只能缩短且须在未来"));
        }
    }
    let mut properties = before.resource.properties.clone();
    properties["expires_at"] = json!(expires);
    let fields = storage::fields(&before.resource, next, properties);
    let saved = storage::write(
        conn,
        &WriteScope {
            tenant: &p.tenant_id,
            actor: &p.actor_id,
            kind: "service_subscription",
            id: &p.subscription_id,
            operation: p.decision.as_str(),
        },
        &fields,
        Some(&before.resource),
    )
    .await?;
    let changed=sqlx::query("UPDATE service_subscription SET resource_version=$1,status=$2,expires_at=$3,
        approved_by=CASE WHEN $2='approved' THEN $4 ELSE approved_by END,approved_at=CASE WHEN $2='approved' THEN $5 ELSE approved_at END,
        revoked_by=CASE WHEN $2='revoked' THEN $4 ELSE revoked_by END,revoked_at=CASE WHEN $2='revoked' THEN $5 ELSE revoked_at END
        WHERE tenant_id=$6 AND subscription_id=$7 AND resource_version=$8")
        .bind(saved.version).bind(next).bind(expires).bind(&p.actor_id).bind(now).bind(&p.tenant_id).bind(&p.subscription_id).bind(before.resource.version)
        .execute(&mut *conn).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("订阅审批出现并发修改"));
    }
    let result = ServiceSubscriptionDecisionResult {
        subscription: saved,
        decision_id: uuid::Uuid::new_v4().to_string(),
        replayed: false,
    };
    sqlx::query("INSERT INTO service_subscription_decision(id,tenant_id,subscription_id,actor_id,action,idempotency_key,semantics,result,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&result.decision_id).bind(&p.tenant_id).bind(&p.subscription_id).bind(&p.actor_id).bind(p.decision.as_str()).bind(&p.idempotency_key)
        .bind(semantic.to_string()).bind(serde_json::to_string(&result).map_err(|_|invalid("服务审批回执编码失败"))?).bind(now).execute(conn).await.map_err(error)?;
    Ok(result)
}

async fn receipt(
    conn: &mut PgConnection,
    p: &DecideServiceSubscriptionParams,
    semantic: &Value,
) -> Result<Option<ServiceSubscriptionDecisionResult>, BackendError> {
    let row=sqlx::query("SELECT actor_id,action,semantics,result FROM service_subscription_decision WHERE tenant_id=$1 AND subscription_id=$2 AND idempotency_key=$3")
        .bind(&p.tenant_id).bind(&p.subscription_id).bind(&p.idempotency_key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let original: Value =
        serde_json::from_str(&row.try_get::<String, _>("semantics").map_err(error)?)
            .map_err(|_| invalid("服务审批原始语义无效"))?;
    if original != *semantic
        || row.try_get::<String, _>("actor_id").map_err(error)? != p.actor_id
        || row.try_get::<String, _>("action").map_err(error)? != p.decision.as_str()
    {
        return Err(conflict("相同幂等键不能处理不同用户或审批语义"));
    }
    let mut result: ServiceSubscriptionDecisionResult =
        serde_json::from_str(&row.try_get::<String, _>("result").map_err(error)?)
            .map_err(|_| invalid("服务审批回执无效"))?;
    result.replayed = true;
    Ok(Some(result))
}
