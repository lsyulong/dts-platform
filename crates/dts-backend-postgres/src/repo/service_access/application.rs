use dts_backend::{
    BackendError, ProductResource, ResourceWrite,
    repo::service_access::{validation, *},
};
use serde_json::json;
use sqlx::{PgConnection, Row};

use super::{
    authority,
    storage::{self, WriteScope, conflict, error, invalid},
};

pub(super) async fn load(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    lock: bool,
) -> Result<Option<ServiceApplication>, BackendError> {
    if lock {
        let owner: Option<String> = sqlx::query_scalar(
            "SELECT owner_id FROM service_application WHERE tenant_id=$1 AND application_id=$2",
        )
        .bind(tenant)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
        if let Some(owner) = owner {
            sqlx::query("SELECT id FROM \"user\" WHERE tenant_id=$1 AND id=$2 FOR SHARE")
                .bind(tenant)
                .bind(owner)
                .fetch_optional(&mut *conn)
                .await
                .map_err(error)?;
        }
    }
    let sql = if lock {
        "SELECT resource_version,owner_id,allowed_origins,credential_expires_at FROM service_application WHERE tenant_id=$1 AND application_id=$2 FOR UPDATE"
    } else {
        "SELECT resource_version,owner_id,allowed_origins,credential_expires_at FROM service_application WHERE tenant_id=$1 AND application_id=$2"
    };
    let row = sqlx::query(sql)
        .bind(tenant)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let resource = storage::get(conn, tenant, "service_application", id).await?;
    let owner_id: String = row.try_get("owner_id").map_err(error)?;
    let origins: Vec<String> =
        serde_json::from_str(&row.try_get::<String, _>("allowed_origins").map_err(error)?)
            .map_err(|_| invalid("应用来源白名单无效"))?;
    let expires = row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("credential_expires_at")
        .map_err(error)?;
    if resource.version != row.try_get::<i64, _>("resource_version").map_err(error)?
        || !matches!(resource.status.as_str(), "active" | "inactive")
        || resource.properties
            != json!({"owner_id":owner_id,"allowed_origins":origins,"credential_expires_at":expires})
    {
        return Err(conflict("应用真实身份与资源版本不一致"));
    }
    validation::origins(&origins)?;
    Ok(Some(ServiceApplication {
        resource,
        owner_id,
        allowed_origins: origins,
        credential_expires_at: expires,
    }))
}

pub(super) async fn required(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
    lock: bool,
) -> Result<ServiceApplication, BackendError> {
    load(conn, tenant, id, lock)
        .await?
        .ok_or_else(|| BackendError::NotFound("实际应用身份不存在".into()))
}

pub(super) async fn save(
    conn: &mut PgConnection,
    params: &SaveServiceApplicationParams,
) -> Result<ProductResource, BackendError> {
    validation::save_application(params)?;
    let actor = authority::get(conn, &params.tenant_id, &params.actor_id, true).await?;
    if !actor.active {
        return Err(invalid("应用操作用户已失效"));
    }
    let before = load(conn, &params.tenant_id, &params.id, true).await?;
    if let Some(app) = &before {
        authority::manage(&actor, &params.actor_id, &app.owner_id)?;
        storage::version(
            &app.resource,
            params
                .expected_version
                .ok_or_else(|| conflict("修改应用须提供版本"))?,
        )?;
        if app.resource.status != "active" {
            return Err(conflict("停用应用不能继续编辑"));
        }
        if app.owner_id != params.owner_id {
            if !actor.can_approve {
                return Err(invalid("只有实际服务管理者可以转交应用"));
            }
            let count:i64=sqlx::query_scalar("SELECT(SELECT COUNT(*) FROM service_credential WHERE tenant_id=$1 AND application_id=$2)
                +(SELECT COUNT(*) FROM service_subscription WHERE tenant_id=$1 AND application_id=$2)")
                .bind(&params.tenant_id).bind(&params.id).fetch_one(&mut *conn).await.map_err(error)?;
            if count != 0 {
                return Err(conflict("已有凭据或订阅历史的应用不能转交所有者"));
            }
        }
    } else {
        if params.expected_version.is_some() {
            return Err(conflict("应用不存在或版本已改变"));
        }
        authority::manage(&actor, &params.actor_id, &params.owner_id)?;
    }
    authority::active(conn, &params.tenant_id, &params.owner_id).await?;
    let expires = storage::canonical_time(params.credential_expires_at)?;
    let mut properties = validation::application_properties(params);
    properties["credential_expires_at"] = json!(expires);
    let fields = ResourceWrite {
        code: params.code.clone(),
        name: params.name.clone(),
        description: params.description.clone(),
        parent_id: None,
        status: "active".into(),
        properties,
        references: vec![],
    };
    let resource = storage::write(
        conn,
        &WriteScope {
            tenant: &params.tenant_id,
            actor: &params.actor_id,
            kind: "service_application",
            id: &params.id,
            operation: if before.is_some() { "update" } else { "create" },
        },
        &fields,
        before.as_ref().map(|a| &a.resource),
    )
    .await?;
    sqlx::query("INSERT INTO service_application(tenant_id,application_id,resource_version,owner_id,owner_status,allowed_origins,credential_expires_at)
        VALUES($1,$2,$3,$4,'active',$5,$6) ON CONFLICT(tenant_id,application_id) DO UPDATE SET resource_version=excluded.resource_version,
        owner_id=excluded.owner_id,owner_status='active',allowed_origins=excluded.allowed_origins,credential_expires_at=excluded.credential_expires_at")
        .bind(&params.tenant_id).bind(&params.id).bind(resource.version).bind(&params.owner_id).bind(json!(params.allowed_origins).to_string()).bind(expires)
        .execute(conn).await.map_err(error)?;
    Ok(resource)
}

pub(super) async fn archive(
    conn: &mut PgConnection,
    params: &ArchiveServiceApplicationParams,
) -> Result<ProductResource, BackendError> {
    validation::version(params.expected_version)?;
    let actor = authority::get(conn, &params.tenant_id, &params.actor_id, true).await?;
    let before = required(conn, &params.tenant_id, &params.application_id, true).await?;
    authority::manage(&actor, &params.actor_id, &before.owner_id)?;
    storage::version(&before.resource, params.expected_version)?;
    if before.resource.status == "inactive" {
        return Ok(before.resource);
    }
    let fields = storage::fields(
        &before.resource,
        "inactive",
        before.resource.properties.clone(),
    );
    let resource = storage::write(
        conn,
        &WriteScope {
            tenant: &params.tenant_id,
            actor: &params.actor_id,
            kind: "service_application",
            id: &params.application_id,
            operation: "archive",
        },
        &fields,
        Some(&before.resource),
    )
    .await?;
    sqlx::query("UPDATE service_application SET resource_version=$1 WHERE tenant_id=$2 AND application_id=$3")
        .bind(resource.version).bind(&params.tenant_id).bind(&params.application_id).execute(conn).await.map_err(error)?;
    Ok(resource)
}
