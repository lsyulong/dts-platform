use super::{PgDataAccessRepo, authority, storage};
use dts_backend::{BackendError, repo::data_access::*};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

pub(super) async fn decision(
    conn: &mut PgConnection,
    params: &DecideDataAccessParams,
    result: &DataAccessDecisionResult,
) -> Result<(), BackendError> {
    let evidence = json!({"request_id":params.request_id,"request_version":result.request.version,"decision_id":result.decision_id,
        "grant_ids":result.grant.as_ref().map(|grant|vec![grant.id.clone()]).unwrap_or_default(),
        "metadata_ids":[result.request.properties["object_id"]],"columns":result.request.properties["columns"]});
    write(conn, &uuid::Uuid::new_v4().to_string(), params, evidence).await
}
async fn write(
    conn: &mut PgConnection,
    id: &str,
    params: &DecideDataAccessParams,
    evidence: Value,
) -> Result<(), BackendError> {
    let now = storage::now(conn).await?;
    sqlx::query("INSERT INTO data_access_audit(id,tenant_id,actor_id,action,outcome,evidence,created_at) VALUES($1,$2,$3,$4,'succeeded',$5,$6)")
        .bind(id).bind(&params.tenant_id).bind(&params.actor_id).bind(params.decision.as_str()).bind(evidence.to_string()).bind(now.to_rfc3339())
        .execute(conn).await.map_err(storage::error)?;
    Ok(())
}
pub(super) async fn record(
    repo: &PgDataAccessRepo,
    params: &RecordDataAccessParams,
) -> Result<(), BackendError> {
    validation::evidence(params)?;
    let mut tx = repo.begin(&params.tenant_id, &params.actor_id).await?;
    for id in &params.evidence.grant_ids {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT id FROM data_access_grant WHERE tenant_id=$1 AND grantee_id=$2 AND id=$3",
        )
        .bind(&params.tenant_id)
        .bind(&params.actor_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage::error)?;
        if found.is_none() {
            return Err(storage::invalid("访问证据中的授权不属于当前账号"));
        }
    }
    for id in &params.evidence.metadata_ids {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT object_id FROM metadata_catalog WHERE tenant_id=$1 AND object_id=$2",
        )
        .bind(&params.tenant_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage::error)?;
        if found.is_none() {
            return Err(storage::invalid("访问证据中的元数据不属于当前租户"));
        }
    }
    let encoded = json!(params.evidence).to_string();
    if encoded.len() > 512 * 1024 {
        return Err(storage::invalid("访问审计证据超过512KiB"));
    }
    let now = storage::now(&mut tx).await?;
    sqlx::query("INSERT INTO data_access_audit(id,tenant_id,actor_id,action,outcome,evidence,created_at) VALUES($1,$2,$3,'query',$4,$5,$6)")
        .bind(&params.id).bind(&params.tenant_id).bind(&params.actor_id).bind(params.outcome.as_str()).bind(encoded).bind(now.to_rfc3339())
        .execute(&mut *tx).await.map_err(storage::error)?;
    tx.commit().await.map_err(storage::error)?;
    Ok(())
}
pub(super) async fn list(
    repo: &PgDataAccessRepo,
    filter: &DataAccessAuditFilter,
) -> Result<DataAccessAuditPage, BackendError> {
    let mut tx = repo.read_begin().await?;
    let authority = authority::get(&mut tx, &filter.tenant_id, &filter.actor_id, false).await?;
    if !authority.active {
        return Err(storage::invalid("账号已失效"));
    }
    let owner = if authority.can_approve {
        None
    } else {
        Some(&filter.actor_id)
    };
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM data_access_audit WHERE tenant_id=$1 AND ($2 IS NULL OR actor_id=$2)",
    )
    .bind(&filter.tenant_id)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .map_err(storage::error)?;
    let rows=sqlx::query("SELECT * FROM data_access_audit WHERE tenant_id=$1 AND ($2 IS NULL OR actor_id=$2) ORDER BY created_at DESC,id LIMIT $3 OFFSET $4")
        .bind(&filter.tenant_id).bind(owner).bind(i64::from(filter.limit.clamp(1,100))).bind(i64::from(filter.offset)).fetch_all(&mut *tx).await.map_err(storage::error)?;
    let data = rows
        .iter()
        .map(|row| {
            Ok(DataAccessAudit {
                id: row.try_get("id").map_err(storage::error)?,
                tenant_id: row.try_get("tenant_id").map_err(storage::error)?,
                actor_id: row.try_get("actor_id").map_err(storage::error)?,
                action: row.try_get("action").map_err(storage::error)?,
                outcome: row.try_get("outcome").map_err(storage::error)?,
                evidence: serde_json::from_str(
                    &row.try_get::<String, _>("evidence")
                        .map_err(storage::error)?,
                )
                .map_err(|_| storage::invalid("审计证据损坏"))?,
                created_at: storage::time(row, "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    tx.commit().await.map_err(storage::error)?;
    Ok(DataAccessAuditPage { data, total })
}
