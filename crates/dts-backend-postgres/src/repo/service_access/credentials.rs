use dts_backend::{
    BackendError,
    repo::service_access::{validation, *},
};
use serde_json::json;
use sqlx::{PgConnection, Row, postgres::PgRow};

use super::{
    application, authority,
    receipts::{self, ReceiptScope},
    storage::{self, WriteScope, conflict, error, invalid},
};

pub(super) fn decode(row: &PgRow) -> Result<ServiceCredentialRecord, BackendError> {
    Ok(ServiceCredentialRecord {
        summary: ServiceCredentialSummary {
            id: row.try_get("id").map_err(error)?,
            tenant_id: row.try_get("tenant_id").map_err(error)?,
            application_id: row.try_get("application_id").map_err(error)?,
            owner_id: row.try_get("owner_id").map_err(error)?,
            version: row.try_get("version").map_err(error)?,
            created_by: row.try_get("created_by").map_err(error)?,
            created_at: row.try_get("created_at").map_err(error)?,
            expires_at: row.try_get("expires_at").map_err(error)?,
            revoked_at: row.try_get("revoked_at").map_err(error)?,
            revoked_by: row.try_get("revoked_by").map_err(error)?,
        },
        verifier: row.try_get("verifier").map_err(error)?,
    })
}
pub(super) async fn lookup(
    conn: &mut PgConnection,
    id: &str,
) -> Result<Option<ServiceCredentialRecord>, BackendError> {
    validation::bounded(id, 128)?;
    sqlx::query("SELECT * FROM service_credential WHERE id=$1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode)
        .transpose()
}
pub(super) async fn locked(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<ServiceCredentialRecord, BackendError> {
    sqlx::query("SELECT * FROM service_credential WHERE tenant_id=$1 AND id=$2 FOR UPDATE")
        .bind(tenant)
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode)
        .transpose()?
        .ok_or_else(|| invalid("当前租户实际服务凭据不存在"))
}

pub(super) async fn issue(
    conn: &mut PgConnection,
    p: &IssueServiceCredentialParams,
) -> Result<ServiceCredentialIssueResult, BackendError> {
    let semantic = validation::issue_semantics(p)?;
    let authority = authority::get(conn, &p.tenant_id, &p.actor_id, true).await?;
    let app = application::required(conn, &p.tenant_id, &p.application_id, true).await?;
    authority::manage(&authority, &p.actor_id, &app.owner_id)?;
    let receipt = ReceiptScope {
        tenant: &p.tenant_id,
        application: &p.application_id,
        actor: &p.actor_id,
        action: "issue_credential",
        key: &p.idempotency_key,
    };
    if let Some(value) = receipts::credential(conn, &receipt, &semantic).await? {
        let mut result: ServiceCredentialIssueResult =
            serde_json::from_value(value).map_err(|_| invalid("服务凭据回执无效"))?;
        result.replayed = true;
        return Ok(result);
    }
    validation::bounded(&p.credential_id, 128)?;
    validation::verifier(&p.verifier)?;
    validation::expiry(p.expires_at)?;
    authority::active(conn, &p.tenant_id, &app.owner_id).await?;
    storage::version(&app.resource, p.expected_version)?;
    if app.resource.status != "active" {
        return Err(conflict("服务应用已停用"));
    }
    let now = storage::now(conn).await?;
    let expires = storage::canonical_time(p.expires_at)?;
    if expires <= now || expires > app.credential_expires_at {
        return Err(invalid("凭据有效期超过应用约束或已过期"));
    }
    sqlx::query("INSERT INTO service_credential(id,tenant_id,application_id,owner_id,version,verifier,created_by,created_at,expires_at)
        VALUES($1,$2,$3,$4,1,$5,$6,$7,$8)")
        .bind(&p.credential_id).bind(&p.tenant_id).bind(&p.application_id).bind(&app.owner_id).bind(&p.verifier).bind(&p.actor_id).bind(now).bind(expires)
        .execute(&mut *conn).await.map_err(error)?;
    let resource = bump_application(conn, &app, &p.actor_id, "issue_credential").await?;
    let credential = locked(conn, &p.tenant_id, &p.credential_id).await?.summary;
    let result = ServiceCredentialIssueResult {
        application: resource,
        credential,
        replayed: false,
    };
    let encoded = serde_json::to_value(&result).map_err(|_| invalid("凭据回执无法编码"))?;
    receipts::save_credential(conn, &receipt, &semantic, &encoded).await?;
    Ok(result)
}

pub(super) async fn revoke(
    conn: &mut PgConnection,
    p: &RevokeServiceCredentialParams,
) -> Result<ServiceCredentialSummary, BackendError> {
    let semantic = validation::revoke_semantics(p)?;
    let authority = authority::get(conn, &p.tenant_id, &p.actor_id, true).await?;
    let app = application::required(conn, &p.tenant_id, &p.application_id, true).await?;
    authority::manage(&authority, &p.actor_id, &app.owner_id)?;
    let receipt = ReceiptScope {
        tenant: &p.tenant_id,
        application: &p.application_id,
        actor: &p.actor_id,
        action: "revoke_credential",
        key: &p.idempotency_key,
    };
    if let Some(value) = receipts::credential(conn, &receipt, &semantic).await? {
        return serde_json::from_value(value).map_err(|_| invalid("凭据撤销回执无效"));
    }
    let credential = locked(conn, &p.tenant_id, &p.credential_id).await?;
    if credential.summary.application_id != p.application_id {
        return Err(invalid("凭据不属于当前应用"));
    }
    if credential.summary.version != p.expected_version || credential.summary.revoked_at.is_some() {
        return Err(conflict("凭据已撤销或版本已变化"));
    }
    let now = storage::now(conn).await?;
    let changed = sqlx::query(
        "UPDATE service_credential SET version=version+1,revoked_at=$1,revoked_by=$2
        WHERE tenant_id=$3 AND application_id=$4 AND id=$5 AND version=$6 AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(&p.actor_id)
    .bind(&p.tenant_id)
    .bind(&p.application_id)
    .bind(&p.credential_id)
    .bind(p.expected_version)
    .execute(&mut *conn)
    .await
    .map_err(error)?
    .rows_affected();
    if changed != 1 {
        return Err(conflict("凭据发生并发撤销"));
    }
    bump_application(conn, &app, &p.actor_id, "revoke_credential").await?;
    let result = locked(conn, &p.tenant_id, &p.credential_id).await?.summary;
    receipts::save_credential(conn, &receipt, &semantic, &json!(result)).await?;
    Ok(result)
}

async fn bump_application(
    conn: &mut PgConnection,
    app: &ServiceApplication,
    actor: &str,
    operation: &str,
) -> Result<dts_backend::ProductResource, BackendError> {
    let fields = storage::fields(
        &app.resource,
        &app.resource.status,
        app.resource.properties.clone(),
    );
    let saved = storage::write(
        conn,
        &WriteScope {
            tenant: &app.resource.tenant_id,
            actor,
            kind: "service_application",
            id: &app.resource.id,
            operation,
        },
        &fields,
        Some(&app.resource),
    )
    .await?;
    sqlx::query("UPDATE service_application SET resource_version=$1 WHERE tenant_id=$2 AND application_id=$3")
        .bind(saved.version).bind(&saved.tenant_id).bind(&saved.id).execute(conn).await.map_err(error)?;
    Ok(saved)
}

pub(super) async fn list(
    conn: &mut PgConnection,
    f: &ServiceCredentialFilter,
) -> Result<ServiceCredentialPage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let authority = authority::get(conn, &f.tenant_id, &f.actor_id, false).await?;
    let app = application::required(conn, &f.tenant_id, &f.application_id, false).await?;
    authority::manage(&authority, &f.actor_id, &app.owner_id)?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM service_credential WHERE tenant_id=$1 AND application_id=$2",
    )
    .bind(&f.tenant_id)
    .bind(&f.application_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(error)?;
    let rows=sqlx::query("SELECT * FROM service_credential WHERE tenant_id=$1 AND application_id=$2 ORDER BY created_at DESC,id LIMIT $3 OFFSET $4")
        .bind(&f.tenant_id).bind(&f.application_id).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(conn).await.map_err(error)?;
    let data = rows
        .iter()
        .map(|r| decode(r).map(|r| r.summary))
        .collect::<Result<_, _>>()?;
    Ok(ServiceCredentialPage { data, total })
}
