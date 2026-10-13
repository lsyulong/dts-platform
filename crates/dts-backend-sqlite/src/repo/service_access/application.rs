use super::{SqliteServiceAccessRepo, authority, storage::*};
use dts_backend::{BackendError, ProductResource, ResourcePage, repo::service_access::*};
use serde_json::json;
use sqlx::{Row, SqliteConnection};

pub(super) async fn get(
    conn: &mut SqliteConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<ServiceApplication>, BackendError> {
    let row =
        sqlx::query("SELECT * FROM service_application WHERE tenant_id=?1 AND application_id=?2")
            .bind(tenant)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let resource = resource(conn, tenant, "service_application", id)
        .await?
        .ok_or_else(|| conflict("服务应用真实定义缺失"))?;
    let app = ServiceApplication {
        resource,
        owner_id: row.try_get("owner_id").map_err(error)?,
        allowed_origins: serde_json::from_str(
            &row.try_get::<String, _>("allowed_origins").map_err(error)?,
        )
        .map_err(|_| invalid("服务来源列表无效"))?,
        credential_expires_at: time(&row, "credential_expires_at")?,
    };
    if app.resource.version != row.try_get::<i64, _>("resource_version").map_err(error)?
        || app.resource.properties
            != json!({"owner_id":app.owner_id,
        "allowed_origins":app.allowed_origins,"credential_expires_at":app.credential_expires_at})
        || !matches!(app.resource.status.as_str(), "active" | "inactive")
    {
        return Err(conflict("服务应用typed定义与真实资源不一致"));
    }
    Ok(Some(app))
}
pub(super) async fn save(
    repo: &SqliteServiceAccessRepo,
    p: &SaveServiceApplicationParams,
) -> Result<ProductResource, BackendError> {
    validation::save_application(p)?;
    let mut tx = repo.begin().await?;
    let actor = authority::active(&mut tx, &p.tenant_id, &p.actor_id).await?;
    authority::active(&mut tx, &p.tenant_id, &p.owner_id).await?;
    if p.actor_id != p.owner_id && !actor.can_approve {
        return Err(invalid("应用负责人不得越权指定"));
    }
    let previous = get(&mut tx, &p.tenant_id, &p.id).await?;
    match (&previous, p.expected_version) {
        (Some(app), Some(version)) => {
            authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &app.owner_id).await?;
            if app.resource.version != version || app.resource.status != "active" {
                return Err(conflict("应用版本已变化或应用已停用"));
            }
            if p.owner_id != app.owner_id {
                let count:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM service_credential WHERE tenant_id=?1 AND application_id=?2)+(SELECT COUNT(*) FROM service_subscription WHERE tenant_id=?1 AND application_id=?2)")
                    .bind(&p.tenant_id).bind(&p.id).fetch_one(&mut *tx).await.map_err(error)?;
                if count != 0 {
                    return Err(conflict("已有凭据或订阅历史的应用负责人不可变更"));
                }
            }
        },
        (None, None) => {},
        _ => return Err(conflict("服务应用创建或更新状态不匹配")),
    }
    let now = now(&mut tx).await?;
    let mut value = previous
        .as_ref()
        .map(|app| app.resource.clone())
        .unwrap_or(ProductResource {
            id: p.id.clone(),
            tenant_id: p.tenant_id.clone(),
            kind: "service_application".into(),
            code: p.code.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            parent_id: None,
            status: "active".into(),
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
    value.properties = validation::application_properties(p);
    // The typed owner FK permits historical soft deletion and is checked active
    // on every use. Generic active-user links are unsuitable for this history.
    value.references = vec![];
    write(
        &mut tx,
        &p.actor_id,
        previous.as_ref().map(|app| &app.resource),
        &value,
    )
    .await?;
    sqlx::query("INSERT INTO service_application(tenant_id,application_id,resource_version,owner_id,owner_status,allowed_origins,credential_expires_at) VALUES(?1,?2,?3,?4,'active',?5,?6) ON CONFLICT(tenant_id,application_id) DO UPDATE SET resource_version=excluded.resource_version,owner_id=excluded.owner_id,owner_status=excluded.owner_status,allowed_origins=excluded.allowed_origins,credential_expires_at=excluded.credential_expires_at")
        .bind(&p.tenant_id).bind(&p.id).bind(value.version).bind(&p.owner_id).bind(json!(p.allowed_origins).to_string()).bind(p.credential_expires_at.to_rfc3339())
        .execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(value)
}
pub(super) async fn archive(
    repo: &SqliteServiceAccessRepo,
    p: &ArchiveServiceApplicationParams,
) -> Result<ProductResource, BackendError> {
    validation::version(p.expected_version)?;
    let mut tx = repo.begin().await?;
    let app = get(&mut tx, &p.tenant_id, &p.application_id)
        .await?
        .ok_or_else(|| invalid("服务应用不存在"))?;
    authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &app.owner_id).await?;
    if app.resource.version != p.expected_version {
        return Err(conflict("应用版本已改变"));
    }
    if app.resource.status == "inactive" {
        return Ok(app.resource);
    }
    let mut value = app.resource.clone();
    bump(&mut value, now(&mut tx).await?)?;
    value.status = "inactive".into();
    write(&mut tx, &p.actor_id, Some(&app.resource), &value).await?;
    sqlx::query("UPDATE service_application SET resource_version=?1 WHERE tenant_id=?2 AND application_id=?3")
        .bind(value.version).bind(&p.tenant_id).bind(&p.application_id).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(value)
}
pub(super) async fn list(
    repo: &SqliteServiceAccessRepo,
    f: &ServiceResourceFilter,
) -> Result<ResourcePage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    if f.approvals {
        return Err(invalid("应用列表不支持订阅审批模式"));
    }
    let pattern = validation::search_pattern(f.q.as_deref())?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    let actor = authority::active(&mut tx, &f.tenant_id, &f.actor_id).await?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT a.application_id FROM service_application a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.kind='service_application' AND r.id=a.application_id WHERE a.tenant_id=?1 AND (a.owner_id=?2 OR ?3=1) AND (?4 IS NULL OR r.status=?4) AND (?5 IS NULL OR r.code LIKE ?5 ESCAPE '\\' OR r.name LIKE ?5 ESCAPE '\\') ORDER BY r.updated_at DESC,a.application_id LIMIT ?6 OFFSET ?7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.status).bind(&pattern).bind(f.limit as i64).bind(f.offset as i64).fetch_all(&mut *tx).await.map_err(error)?;
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM service_application a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.kind='service_application' AND r.id=a.application_id WHERE a.tenant_id=?1 AND (a.owner_id=?2 OR ?3=1) AND (?4 IS NULL OR r.status=?4) AND (?5 IS NULL OR r.code LIKE ?5 ESCAPE '\\' OR r.name LIKE ?5 ESCAPE '\\')")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.status).bind(&pattern).fetch_one(&mut *tx).await.map_err(error)?;
    let mut data = Vec::new();
    for id in ids {
        data.push(
            get(&mut tx, &f.tenant_id, &id)
                .await?
                .ok_or_else(|| conflict("服务应用分页状态变化"))?
                .resource,
        );
    }
    tx.commit().await.map_err(error)?;
    Ok(ResourcePage { data, total })
}
