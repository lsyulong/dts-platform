use dts_backend::{
    BackendError, ProductResource, ResourceReference, ResourceWrite,
    repo::service_access::{validation, *},
};
use serde_json::json;
use sqlx::{PgConnection, Row};

use super::{
    application, authority,
    storage::{self, WriteScope, conflict, error, invalid},
};

pub(super) async fn api(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    version: i64,
    lock: bool,
) -> Result<ProductResource, BackendError> {
    if lock {
        sqlx::query("SELECT id FROM product_resource WHERE tenant_id=$1 AND kind='data_api' AND id=$2 FOR SHARE")
            .bind(tenant).bind(id).fetch_optional(&mut *conn).await.map_err(error)?;
    }
    let value = storage::get(conn, tenant, "data_api", id).await?;
    if value.status != "published" || value.version != version {
        return Err(conflict("数据API已撤回或发布版本改变"));
    }
    let publication = &value.properties["publication"];
    if publication["format_version"] != 1
        || publication["executor"] != "data_api_detail_v1"
        || publication["tenant_id"] != tenant
        || publication["source_id"] != id
        || publication["source_version"] != version
        || publication["api"]["rate_limit"] != value.properties["rate_limit"]
    {
        return Err(conflict("数据API没有当前实际冻结发布证明"));
    }
    Ok(value)
}

pub(super) async fn load(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    lock: bool,
) -> Result<Option<ServiceSubscription>, BackendError> {
    let sql = if lock {
        "SELECT * FROM service_subscription WHERE tenant_id=$1 AND subscription_id=$2 FOR UPDATE"
    } else {
        "SELECT * FROM service_subscription WHERE tenant_id=$1 AND subscription_id=$2"
    };
    let row = sqlx::query(sql)
        .bind(tenant)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let value = ServiceSubscription {
        resource: storage::get(conn, tenant, "service_subscription", id).await?,
        application_id: row.try_get("application_id").map_err(error)?,
        owner_id: row.try_get("owner_id").map_err(error)?,
        requester_id: row.try_get("requester_id").map_err(error)?,
        api_id: row.try_get("api_id").map_err(error)?,
        api_version: row.try_get("api_version").map_err(error)?,
        purpose: row.try_get("purpose").map_err(error)?,
        expires_at: row.try_get("expires_at").map_err(error)?,
        approved_by: row.try_get("approved_by").map_err(error)?,
        approved_at: row.try_get("approved_at").map_err(error)?,
        revoked_by: row.try_get("revoked_by").map_err(error)?,
        revoked_at: row.try_get("revoked_at").map_err(error)?,
    };
    if value.resource.version != row.try_get::<i64, _>("resource_version").map_err(error)?
        || value.resource.status != row.try_get::<String, _>("status").map_err(error)?
        || value.resource.properties
            != json!({"application_id":value.application_id,"api_id":value.api_id,"api_version":value.api_version,
            "owner_id":value.owner_id,"requester_id":value.requester_id,"purpose":value.purpose,"expires_at":value.expires_at})
    {
        return Err(conflict("真实订阅定义与资源版本不一致"));
    }
    Ok(Some(value))
}

pub(super) async fn required(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    lock: bool,
) -> Result<ServiceSubscription, BackendError> {
    load(conn, tenant, id, lock)
        .await?
        .ok_or_else(|| BackendError::NotFound("实际服务订阅不存在".into()))
}

pub(super) async fn save(
    conn: &mut PgConnection,
    p: &SaveServiceSubscriptionParams,
) -> Result<ProductResource, BackendError> {
    validation::save_subscription(p)?;
    let actor = authority::get(conn, &p.tenant_id, &p.actor_id, true).await?;
    let app = application::required(conn, &p.tenant_id, &p.application_id, true).await?;
    authority::manage(&actor, &p.actor_id, &app.owner_id)?;
    authority::active(conn, &p.tenant_id, &app.owner_id).await?;
    if app.resource.status != "active" {
        return Err(invalid("订阅应用已停用"));
    }
    api(conn, &p.tenant_id, &p.api_id, p.api_version, true).await?;
    let previous = load(conn, &p.tenant_id, &p.id, true).await?;
    match (&previous, p.expected_version) {
        (Some(sub), Some(version)) => {
            authority::manage(&actor, &p.actor_id, &sub.owner_id)?;
            storage::version(&sub.resource, version)?;
            if !matches!(sub.resource.status.as_str(), "draft" | "rejected") {
                return Err(conflict("只能编辑草稿或驳回订阅"));
            }
        },
        (None, None) => {},
        _ => return Err(conflict("订阅创建或修改版本不一致")),
    }
    let requester = previous
        .as_ref()
        .map(|s| s.requester_id.as_str())
        .unwrap_or(&p.actor_id);
    let expires = storage::canonical_time(p.expires_at)?;
    let mut properties = validation::subscription_properties(p, &app.owner_id, requester);
    properties["expires_at"] = json!(expires);
    let fields = ResourceWrite {
        code: p.code.clone(),
        name: p.name.clone(),
        description: p.description.clone(),
        parent_id: None,
        status: "draft".into(),
        properties,
        references: vec![
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
        ],
    };
    let saved = storage::write(
        conn,
        &WriteScope {
            tenant: &p.tenant_id,
            actor: &p.actor_id,
            kind: "service_subscription",
            id: &p.id,
            operation: if previous.is_some() {
                "update"
            } else {
                "create"
            },
        },
        &fields,
        previous.as_ref().map(|s| &s.resource),
    )
    .await?;
    sqlx::query("INSERT INTO service_subscription(tenant_id,subscription_id,resource_version,application_id,owner_id,requester_id,api_id,api_version,status,purpose,expires_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,'draft',$9,$10) ON CONFLICT(tenant_id,subscription_id) DO UPDATE SET resource_version=excluded.resource_version,
        application_id=excluded.application_id,owner_id=excluded.owner_id,api_id=excluded.api_id,api_version=excluded.api_version,status='draft',purpose=excluded.purpose,
        expires_at=excluded.expires_at,approved_by=NULL,approved_at=NULL,revoked_by=NULL,revoked_at=NULL")
        .bind(&p.tenant_id).bind(&p.id).bind(saved.version).bind(&p.application_id).bind(&app.owner_id).bind(requester).bind(&p.api_id).bind(p.api_version)
        .bind(&p.purpose).bind(expires).execute(conn).await.map_err(error)?;
    Ok(saved)
}
