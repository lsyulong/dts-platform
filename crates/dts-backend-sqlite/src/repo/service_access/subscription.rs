use super::{SqliteServiceAccessRepo, application, authority, storage::*};
use dts_backend::{
    BackendError, ProductResource, ResourcePage, ResourceReference, repo::service_access::*,
};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection};

pub(super) async fn api(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
    version: i64,
) -> Result<ProductResource, BackendError> {
    let value = resource(conn, tenant, "data_api", id)
        .await?
        .filter(|api| api.status == "published" && api.version == version)
        .ok_or_else(|| conflict("数据API已撤回或发布版本发生变化"))?;
    let publication = &value.properties["publication"];
    if publication["format_version"] != 1
        || publication["executor"] != "data_api_detail_v1"
        || publication["tenant_id"] != tenant
        || publication["source_id"] != id
        || publication["source_version"] != version
        || publication["api"]["rate_limit"] != value.properties["rate_limit"]
    {
        return Err(conflict("数据API缺少当前真实冻结发布版本"));
    }
    Ok(value)
}
pub(super) async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<ServiceSubscription>, BackendError> {
    let row =
        sqlx::query("SELECT * FROM service_subscription WHERE tenant_id=?1 AND subscription_id=?2")
            .bind(tenant)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let value = ServiceSubscription {
        resource: resource(conn, tenant, "service_subscription", id)
            .await?
            .ok_or_else(|| conflict("服务订阅真实定义缺失"))?,
        application_id: row.try_get("application_id").map_err(error)?,
        owner_id: row.try_get("owner_id").map_err(error)?,
        requester_id: row.try_get("requester_id").map_err(error)?,
        api_id: row.try_get("api_id").map_err(error)?,
        api_version: row.try_get("api_version").map_err(error)?,
        purpose: row.try_get("purpose").map_err(error)?,
        expires_at: time(&row, "expires_at")?,
        approved_by: row.try_get("approved_by").map_err(error)?,
        approved_at: optional_time(&row, "approved_at")?,
        revoked_by: row.try_get("revoked_by").map_err(error)?,
        revoked_at: optional_time(&row, "revoked_at")?,
    };
    let props = json!({"application_id":value.application_id,"api_id":value.api_id,"api_version":value.api_version,"owner_id":value.owner_id,
        "requester_id":value.requester_id,"purpose":value.purpose,"expires_at":value.expires_at});
    if value.resource.version != row.try_get::<i64, _>("resource_version").map_err(error)?
        || value.resource.status != row.try_get::<String, _>("status").map_err(error)?
        || value.resource.properties != props
    {
        return Err(conflict("服务订阅typed定义与真实资源不一致"));
    }
    Ok(Some(value))
}
pub(super) async fn save(
    repo: &SqliteServiceAccessRepo,
    p: &SaveServiceSubscriptionParams,
) -> Result<ProductResource, BackendError> {
    validation::save_subscription(p)?;
    let mut tx = repo.begin().await?;
    let app = application::get(&mut tx, &p.tenant_id, &p.application_id)
        .await?
        .filter(|app| app.resource.status == "active")
        .ok_or_else(|| invalid("订阅应用未启用或不存在"))?;
    authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &app.owner_id).await?;
    authority::active(&mut tx, &p.tenant_id, &app.owner_id).await?;
    api(&mut tx, &p.tenant_id, &p.api_id, p.api_version).await?;
    let previous = get(&mut tx, &p.tenant_id, &p.id).await?;
    match (&previous, p.expected_version) {
        (Some(sub), Some(version)) => {
            authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &sub.owner_id).await?;
            if sub.resource.version != version
                || !matches!(sub.resource.status.as_str(), "draft" | "rejected")
            {
                return Err(conflict("仅本人有效草稿或驳回订阅可更新"));
            }
        },
        (None, None) => {},
        _ => return Err(conflict("订阅创建或更新状态不匹配")),
    }
    let now = now(&mut tx).await?;
    let requester = previous
        .as_ref()
        .map(|sub| sub.requester_id.as_str())
        .unwrap_or(&p.actor_id);
    let mut value = previous
        .as_ref()
        .map(|sub| sub.resource.clone())
        .unwrap_or(ProductResource {
            id: p.id.clone(),
            tenant_id: p.tenant_id.clone(),
            kind: "service_subscription".into(),
            code: p.code.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            parent_id: None,
            status: "draft".into(),
            properties: json!({}),
            version: 1,
            created_by: p.actor_id.clone(),
            created_at: now,
            updated_at: now,
            references: vec![],
        });
    if previous.is_some() {
        bump(&mut value, now)?;
    }
    value.code = p.code.clone();
    value.name = p.name.clone();
    value.description = p.description.clone();
    value.status = "draft".into();
    value.properties = validation::subscription_properties(p, &app.owner_id, requester);
    value.references = vec![
        ResourceReference {
            field: "application_id".into(),
            target_kind: "service_application".into(),
            target_id: p.application_id.clone(),
        },
        ResourceReference {
            field: "api_id".into(),
            target_kind: "data_api".into(),
            target_id: p.api_id.clone(),
        },
    ];
    write(
        &mut tx,
        &p.actor_id,
        previous.as_ref().map(|sub| &sub.resource),
        &value,
    )
    .await?;
    sqlx::query("INSERT INTO service_subscription(tenant_id,subscription_id,resource_version,application_id,owner_id,requester_id,api_id,api_version,status,purpose,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'draft',?9,?10) ON CONFLICT(tenant_id,subscription_id) DO UPDATE SET resource_version=excluded.resource_version,application_id=excluded.application_id,owner_id=excluded.owner_id,api_id=excluded.api_id,api_version=excluded.api_version,status='draft',purpose=excluded.purpose,expires_at=excluded.expires_at,approved_by=NULL,approved_at=NULL,revoked_by=NULL,revoked_at=NULL")
        .bind(&p.tenant_id).bind(&p.id).bind(value.version).bind(&p.application_id).bind(&app.owner_id).bind(requester).bind(&p.api_id).bind(p.api_version)
        .bind(&p.purpose).bind(p.expires_at.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(value)
}
pub(super) async fn decide(
    repo: &SqliteServiceAccessRepo,
    p: &DecideServiceSubscriptionParams,
) -> Result<ServiceSubscriptionDecisionResult, BackendError> {
    let semantic = validation::decision_semantics(p)?;
    let mut tx = repo.begin().await?;
    let actor = authority::active(&mut tx, &p.tenant_id, &p.actor_id).await?;
    if p.decision.needs_approver() && !actor.can_approve {
        return Err(invalid("当前用户没有真实services:approve权限"));
    }
    let before = get(&mut tx, &p.tenant_id, &p.subscription_id)
        .await?
        .ok_or_else(|| invalid("真实订阅不存在"))?;
    if !p.decision.needs_approver() {
        authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &before.owner_id).await?;
    }
    if let Some(row)=sqlx::query("SELECT semantics,result FROM service_subscription_decision WHERE tenant_id=?1 AND subscription_id=?2 AND idempotency_key=?3")
        .bind(&p.tenant_id).bind(&p.subscription_id).bind(&p.idempotency_key).fetch_optional(&mut *tx).await.map_err(error)? {
        let existing:Value=serde_json::from_str(&row.try_get::<String,_>("semantics").map_err(error)?).map_err(|_|invalid("审批幂等语义无效"))?;
        if existing!=semantic{return Err(conflict("相同幂等键不能处理不同审批语义"));}
        let mut result:ServiceSubscriptionDecisionResult=serde_json::from_str(&row.try_get::<String,_>("result").map_err(error)?).map_err(|_|invalid("审批回执无效"))?;
        result.replayed=true;tx.commit().await.map_err(error)?;return Ok(result);
    }
    if before.resource.version != p.expected_version {
        return Err(conflict("订阅版本已改变"));
    }
    let next = match p.decision {
        ServiceSubscriptionDecision::Submit if before.resource.status == "draft" => "pending",
        ServiceSubscriptionDecision::Approve if before.resource.status == "pending" => "approved",
        ServiceSubscriptionDecision::Reject if before.resource.status == "pending" => "rejected",
        ServiceSubscriptionDecision::Revoke if before.resource.status == "approved" => "revoked",
        _ => return Err(conflict("订阅当前状态不允许该动作")),
    };
    if p.decision.needs_approver()
        && (p.actor_id == before.owner_id || p.actor_id == before.requester_id)
    {
        return Err(invalid("不能审批本人应用或本人提交的服务申请"));
    }
    let now = now(&mut tx).await?;
    let expires = p.expires_at.unwrap_or(before.expires_at);
    if matches!(
        p.decision,
        ServiceSubscriptionDecision::Submit | ServiceSubscriptionDecision::Approve
    ) {
        let app = application::get(&mut tx, &p.tenant_id, &before.application_id)
            .await?
            .filter(|app| app.resource.status == "active" && app.owner_id == before.owner_id)
            .ok_or_else(|| conflict("订阅应用已停用或负责人变化"))?;
        authority::active(&mut tx, &p.tenant_id, &app.owner_id).await?;
        api(&mut tx, &p.tenant_id, &before.api_id, before.api_version).await?;
        if expires <= now || expires > before.expires_at {
            return Err(invalid("审批有效期只能缩短且须在未来"));
        }
    }
    let mut value = before.resource.clone();
    bump(&mut value, now)?;
    value.status = next.into();
    value.properties["expires_at"] = json!(expires);
    write(&mut tx, &p.actor_id, Some(&before.resource), &value).await?;
    sqlx::query("UPDATE service_subscription SET resource_version=?1,status=?2,expires_at=?3,approved_by=CASE WHEN ?2='approved' THEN ?4 ELSE approved_by END,approved_at=CASE WHEN ?2='approved' THEN ?5 ELSE approved_at END,revoked_by=CASE WHEN ?2='revoked' THEN ?4 ELSE revoked_by END,revoked_at=CASE WHEN ?2='revoked' THEN ?5 ELSE revoked_at END WHERE tenant_id=?6 AND subscription_id=?7 AND resource_version=?8")
        .bind(value.version).bind(next).bind(expires.to_rfc3339()).bind(&p.actor_id).bind(now.to_rfc3339()).bind(&p.tenant_id).bind(&p.subscription_id)
        .bind(before.resource.version).execute(&mut *tx).await.map_err(error)?;
    let result = ServiceSubscriptionDecisionResult {
        subscription: value,
        decision_id: uuid::Uuid::new_v4().to_string(),
        replayed: false,
    };
    sqlx::query("INSERT INTO service_subscription_decision(id,tenant_id,subscription_id,actor_id,action,idempotency_key,semantics,result,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(&result.decision_id).bind(&p.tenant_id).bind(&p.subscription_id).bind(&p.actor_id).bind(p.decision.as_str()).bind(&p.idempotency_key)
        .bind(semantic.to_string()).bind(serde_json::to_string(&result).map_err(|_|invalid("审批回执编码失败"))?).bind(now.to_rfc3339())
        .execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
pub(super) async fn list(
    repo: &SqliteServiceAccessRepo,
    f: &ServiceResourceFilter,
) -> Result<ResourcePage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let pattern = validation::search_pattern(f.q.as_deref())?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    let authority = authority::active(&mut tx, &f.tenant_id, &f.actor_id).await?;
    if f.approvals && !authority.can_approve {
        return Err(invalid("服务审批列表必须具有精确services:approve权限"));
    }
    let ids:Vec<String>=sqlx::query_scalar("SELECT s.subscription_id FROM service_subscription s JOIN product_resource r ON r.tenant_id=s.tenant_id AND r.kind='service_subscription' AND r.id=s.subscription_id WHERE s.tenant_id=?1 AND (s.owner_id=?2 OR ?3=1) AND (?4 IS NULL OR s.status=?4) AND (?5 IS NULL OR r.code LIKE ?5 ESCAPE '\\' OR r.name LIKE ?5 ESCAPE '\\') ORDER BY s.subscription_id LIMIT ?6 OFFSET ?7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(f.approvals).bind(&f.status).bind(&pattern).bind(f.limit as i64).bind(f.offset as i64).fetch_all(&mut *tx).await.map_err(error)?;
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM service_subscription s JOIN product_resource r ON r.tenant_id=s.tenant_id AND r.kind='service_subscription' AND r.id=s.subscription_id WHERE s.tenant_id=?1 AND (s.owner_id=?2 OR ?3=1) AND (?4 IS NULL OR s.status=?4) AND (?5 IS NULL OR r.code LIKE ?5 ESCAPE '\\' OR r.name LIKE ?5 ESCAPE '\\')")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(f.approvals).bind(&f.status).bind(&pattern).fetch_one(&mut *tx).await.map_err(error)?;
    let mut data = Vec::new();
    for id in ids {
        data.push(
            get(&mut tx, &f.tenant_id, &id)
                .await?
                .ok_or_else(|| conflict("订阅分页状态改变"))?
                .resource,
        );
    }
    tx.commit().await.map_err(error)?;
    Ok(ResourcePage { data, total })
}
