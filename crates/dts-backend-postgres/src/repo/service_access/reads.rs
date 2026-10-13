use dts_backend::{
    BackendError, ResourcePage,
    repo::service_access::{validation, *},
};
use sqlx::PgConnection;

use super::{
    application as apps, authority,
    storage::{conflict, error, invalid},
    subscription as subs,
};

pub(super) async fn application(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    id: &str,
) -> Result<Option<ServiceApplication>, BackendError> {
    let authority = authority::get(conn, tenant, actor, false).await?;
    if !authority.active {
        return Err(invalid("应用查看用户已失效"));
    }
    let value = apps::load(conn, tenant, id, false).await?;
    if let Some(app) = &value {
        authority::manage(&authority, actor, &app.owner_id)?;
    }
    Ok(value)
}
pub(super) async fn subscription(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    id: &str,
) -> Result<Option<ServiceSubscription>, BackendError> {
    let authority = authority::get(conn, tenant, actor, false).await?;
    if !authority.active {
        return Err(invalid("服务订阅查看用户已失效"));
    }
    let value = subs::load(conn, tenant, id, false).await?;
    if let Some(sub) = &value {
        authority::manage(&authority, actor, &sub.owner_id)?;
    }
    Ok(value)
}

pub(super) async fn applications(
    conn: &mut PgConnection,
    f: &ServiceResourceFilter,
) -> Result<ResourcePage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let search = validation::search_pattern(f.q.as_deref())?;
    let actor = authority::get(conn, &f.tenant_id, &f.actor_id, false).await?;
    if !actor.active || f.approvals {
        return Err(invalid("应用列表用户无效或不能使用审批标记"));
    }
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM service_application a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.id=a.application_id AND r.kind='service_application'
        WHERE a.tenant_id=$1 AND(a.owner_id=$2 OR $3) AND($4::text IS NULL OR r.status=$4)
        AND($5::text IS NULL OR r.code LIKE $5 ESCAPE '\\' OR r.name LIKE $5 ESCAPE '\\')")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.status).bind(&search).fetch_one(&mut *conn).await.map_err(error)?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT a.application_id FROM service_application a JOIN product_resource r ON r.tenant_id=a.tenant_id AND r.id=a.application_id AND r.kind='service_application'
        WHERE a.tenant_id=$1 AND(a.owner_id=$2 OR $3) AND($4::text IS NULL OR r.status=$4)
        AND($5::text IS NULL OR r.code LIKE $5 ESCAPE '\\' OR r.name LIKE $5 ESCAPE '\\') ORDER BY r.updated_at DESC,r.id LIMIT $6 OFFSET $7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(actor.can_approve).bind(&f.status).bind(&search).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(&mut *conn).await.map_err(error)?;
    let mut data = Vec::with_capacity(ids.len());
    for id in ids {
        data.push(
            apps::load(conn, &f.tenant_id, &id, false)
                .await?
                .ok_or_else(|| conflict("应用分页资源不存在"))?
                .resource,
        );
    }
    Ok(ResourcePage { data, total })
}

pub(super) async fn subscriptions(
    conn: &mut PgConnection,
    f: &ServiceResourceFilter,
) -> Result<ResourcePage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let search = validation::search_pattern(f.q.as_deref())?;
    let actor = authority::get(conn, &f.tenant_id, &f.actor_id, false).await?;
    if !actor.active || (f.approvals && !actor.can_approve) {
        return Err(invalid("审批列表要求实际services:approve权限"));
    }
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM service_subscription s JOIN product_resource r
        ON r.tenant_id=s.tenant_id AND r.id=s.subscription_id AND r.kind='service_subscription'
        WHERE s.tenant_id=$1 AND(s.owner_id=$2 OR $3) AND($4::text IS NULL OR s.status=$4)
        AND($5::text IS NULL OR r.code LIKE $5 ESCAPE '\\' OR r.name LIKE $5 ESCAPE '\\')",
    )
    .bind(&f.tenant_id)
    .bind(&f.actor_id)
    .bind(f.approvals)
    .bind(&f.status)
    .bind(&search)
    .fetch_one(&mut *conn)
    .await
    .map_err(error)?;
    let ids:Vec<String>=sqlx::query_scalar("SELECT s.subscription_id FROM service_subscription s JOIN product_resource r
        ON r.tenant_id=s.tenant_id AND r.id=s.subscription_id AND r.kind='service_subscription'
        WHERE s.tenant_id=$1 AND(s.owner_id=$2 OR $3) AND($4::text IS NULL OR s.status=$4)
        AND($5::text IS NULL OR r.code LIKE $5 ESCAPE '\\' OR r.name LIKE $5 ESCAPE '\\') ORDER BY r.updated_at DESC,r.id LIMIT $6 OFFSET $7")
        .bind(&f.tenant_id).bind(&f.actor_id).bind(f.approvals).bind(&f.status).bind(&search).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(&mut *conn).await.map_err(error)?;
    let mut data = Vec::with_capacity(ids.len());
    for id in ids {
        data.push(
            subs::load(conn, &f.tenant_id, &id, false)
                .await?
                .ok_or_else(|| conflict("订阅分页资源不存在"))?
                .resource,
        );
    }
    Ok(ResourcePage { data, total })
}
