use super::{SqliteServiceAccessRepo, application, authority, storage::*};
use dts_backend::{BackendError, repo::service_access::*};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

fn decode(row: &SqliteRow) -> Result<ServiceCredentialRecord, BackendError> {
    Ok(ServiceCredentialRecord {
        summary: ServiceCredentialSummary {
            id: row.try_get("id").map_err(error)?,
            tenant_id: row.try_get("tenant_id").map_err(error)?,
            application_id: row.try_get("application_id").map_err(error)?,
            owner_id: row.try_get("owner_id").map_err(error)?,
            version: row.try_get("version").map_err(error)?,
            created_by: row.try_get("created_by").map_err(error)?,
            created_at: time(row, "created_at")?,
            expires_at: time(row, "expires_at")?,
            revoked_at: optional_time(row, "revoked_at")?,
            revoked_by: row.try_get("revoked_by").map_err(error)?,
        },
        verifier: row.try_get("verifier").map_err(error)?,
    })
}
pub(super) async fn lookup(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Option<ServiceCredentialRecord>, BackendError> {
    sqlx::query("SELECT * FROM service_credential WHERE id=?1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(error)?
        .as_ref()
        .map(decode)
        .transpose()
}
pub(super) async fn issue(
    repo: &SqliteServiceAccessRepo,
    p: &IssueServiceCredentialParams,
) -> Result<ServiceCredentialIssueResult, BackendError> {
    let semantic = validation::issue_semantics(p)?;
    let mut tx = repo.begin().await?;
    let app = application::get(&mut tx, &p.tenant_id, &p.application_id)
        .await?
        .ok_or_else(|| invalid("服务应用不存在"))?;
    authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &app.owner_id).await?;
    if let Some(result) = receipt(
        &mut tx,
        &p.tenant_id,
        &p.application_id,
        &p.idempotency_key,
        &semantic,
    )
    .await?
    {
        let mut result: ServiceCredentialIssueResult =
            serde_json::from_value(result).map_err(|_| invalid("凭据回执无效"))?;
        result.replayed = true;
        tx.commit().await.map_err(error)?;
        return Ok(result);
    }
    validation::bounded(&p.credential_id, 128)?;
    validation::verifier(&p.verifier)?;
    validation::expiry(p.expires_at)?;
    authority::active(&mut tx, &p.tenant_id, &app.owner_id).await?;
    if app.resource.status != "active" || app.resource.version != p.expected_version {
        return Err(conflict("应用已停用或签发版本改变"));
    }
    let now = now(&mut tx).await?;
    if p.expires_at <= now || p.expires_at > app.credential_expires_at {
        return Err(invalid("凭据有效期超过应用约束或已过期"));
    }
    sqlx::query("INSERT INTO service_credential(id,tenant_id,application_id,owner_id,version,verifier,created_by,created_at,expires_at) VALUES(?1,?2,?3,?4,1,?5,?6,?7,?8)")
        .bind(&p.credential_id).bind(&p.tenant_id).bind(&p.application_id).bind(&app.owner_id).bind(&p.verifier).bind(&p.actor_id)
        .bind(now.to_rfc3339()).bind(p.expires_at.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
    let mut resource = app.resource.clone();
    bump(&mut resource, now)?;
    write(&mut tx, &p.actor_id, Some(&app.resource), &resource).await?;
    sqlx::query("UPDATE service_application SET resource_version=?1 WHERE tenant_id=?2 AND application_id=?3").bind(resource.version)
        .bind(&p.tenant_id).bind(&p.application_id).execute(&mut *tx).await.map_err(error)?;
    let result = ServiceCredentialIssueResult {
        application: resource,
        credential: lookup(&mut tx, &p.credential_id)
            .await?
            .ok_or_else(|| invalid("签发凭据缺失"))?
            .summary,
        replayed: false,
    };
    let receipt_result = serde_json::to_value(&result).map_err(|_| invalid("凭据回执编码失败"))?;
    store_receipt(
        &mut tx,
        &ReceiptRecord {
            tenant: &p.tenant_id,
            application: &p.application_id,
            actor: &p.actor_id,
            action: "issue_credential",
            key: &p.idempotency_key,
            semantic: &semantic,
            result: &receipt_result,
            now,
        },
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
pub(super) async fn revoke(
    repo: &SqliteServiceAccessRepo,
    p: &RevokeServiceCredentialParams,
) -> Result<ServiceCredentialSummary, BackendError> {
    let semantic = validation::revoke_semantics(p)?;
    let mut tx = repo.begin().await?;
    let app = application::get(&mut tx, &p.tenant_id, &p.application_id)
        .await?
        .ok_or_else(|| invalid("服务应用不存在"))?;
    authority::manage(&mut tx, &p.tenant_id, &p.actor_id, &app.owner_id).await?;
    if let Some(result) = receipt(
        &mut tx,
        &p.tenant_id,
        &p.application_id,
        &p.idempotency_key,
        &semantic,
    )
    .await?
    {
        let result = serde_json::from_value(result).map_err(|_| invalid("撤销回执无效"))?;
        tx.commit().await.map_err(error)?;
        return Ok(result);
    }
    let credential = lookup(&mut tx, &p.credential_id)
        .await?
        .filter(|credential| {
            credential.summary.tenant_id == p.tenant_id
                && credential.summary.application_id == p.application_id
        })
        .ok_or_else(|| invalid("凭据不属于当前应用"))?;
    if credential.summary.version != p.expected_version || credential.summary.revoked_at.is_some() {
        return Err(conflict("凭据版本已改变或已经撤销"));
    }
    let now = now(&mut tx).await?;
    let changed=sqlx::query("UPDATE service_credential SET version=version+1,revoked_at=?1,revoked_by=?2 WHERE tenant_id=?3 AND application_id=?4 AND id=?5 AND version=?6 AND revoked_at IS NULL")
        .bind(now.to_rfc3339()).bind(&p.actor_id).bind(&p.tenant_id).bind(&p.application_id).bind(&p.credential_id).bind(p.expected_version)
        .execute(&mut *tx).await.map_err(error)?.rows_affected();
    if changed != 1 {
        return Err(conflict("凭据并发撤销"));
    }
    let mut resource = app.resource.clone();
    bump(&mut resource, now)?;
    write(&mut tx, &p.actor_id, Some(&app.resource), &resource).await?;
    sqlx::query("UPDATE service_application SET resource_version=?1 WHERE tenant_id=?2 AND application_id=?3").bind(resource.version)
        .bind(&p.tenant_id).bind(&p.application_id).execute(&mut *tx).await.map_err(error)?;
    let result = lookup(&mut tx, &p.credential_id)
        .await?
        .ok_or_else(|| invalid("撤销凭据缺失"))?
        .summary;
    let receipt_result = serde_json::to_value(&result).map_err(|_| invalid("凭据回执编码失败"))?;
    store_receipt(
        &mut tx,
        &ReceiptRecord {
            tenant: &p.tenant_id,
            application: &p.application_id,
            actor: &p.actor_id,
            action: "revoke_credential",
            key: &p.idempotency_key,
            semantic: &semantic,
            result: &receipt_result,
            now,
        },
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
pub(super) async fn list(
    repo: &SqliteServiceAccessRepo,
    f: &ServiceCredentialFilter,
) -> Result<ServiceCredentialPage, BackendError> {
    validation::pagination(f.limit, f.offset)?;
    let mut tx = repo.pool.begin().await.map_err(error)?;
    let app = application::get(&mut tx, &f.tenant_id, &f.application_id)
        .await?
        .ok_or_else(|| invalid("服务应用不存在"))?;
    authority::manage(&mut tx, &f.tenant_id, &f.actor_id, &app.owner_id).await?;
    let rows=sqlx::query("SELECT * FROM service_credential WHERE tenant_id=?1 AND application_id=?2 ORDER BY created_at DESC,id LIMIT ?3 OFFSET ?4")
        .bind(&f.tenant_id).bind(&f.application_id).bind(f.limit as i64).bind(f.offset as i64).fetch_all(&mut *tx).await.map_err(error)?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM service_credential WHERE tenant_id=?1 AND application_id=?2",
    )
    .bind(&f.tenant_id)
    .bind(&f.application_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(error)?;
    let data = rows
        .iter()
        .map(|row| decode(row).map(|value| value.summary))
        .collect::<Result<_, _>>()?;
    tx.commit().await.map_err(error)?;
    Ok(ServiceCredentialPage { data, total })
}
async fn receipt(
    conn: &mut SqliteConnection,
    tenant: &str,
    app: &str,
    key: &str,
    semantic: &Value,
) -> Result<Option<Value>, BackendError> {
    let row=sqlx::query("SELECT semantics,result FROM service_credential_receipt WHERE tenant_id=?1 AND application_id=?2 AND idempotency_key=?3")
        .bind(tenant).bind(app).bind(key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let existing: Value =
        serde_json::from_str(&row.try_get::<String, _>("semantics").map_err(error)?)
            .map_err(|_| invalid("凭据幂等语义无效"))?;
    if existing != *semantic {
        return Err(conflict("相同幂等键不能签发或撤销不同凭据语义"));
    }
    serde_json::from_str(&row.try_get::<String, _>("result").map_err(error)?)
        .map(Some)
        .map_err(|_| invalid("凭据回执无效"))
}
struct ReceiptRecord<'a> {
    tenant: &'a str,
    application: &'a str,
    actor: &'a str,
    action: &'a str,
    key: &'a str,
    semantic: &'a Value,
    result: &'a Value,
    now: chrono::DateTime<chrono::Utc>,
}

async fn store_receipt(
    conn: &mut SqliteConnection,
    record: &ReceiptRecord<'_>,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO service_credential_receipt(tenant_id,application_id,idempotency_key,actor_id,action,semantics,result,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
        .bind(record.tenant).bind(record.application).bind(record.key).bind(record.actor).bind(record.action).bind(record.semantic.to_string()).bind(record.result.to_string()).bind(record.now.to_rfc3339())
        .execute(conn).await.map_err(error)?;
    Ok(())
}
