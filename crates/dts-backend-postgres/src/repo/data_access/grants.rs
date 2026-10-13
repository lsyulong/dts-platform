use super::{PgDataAccessRepo, storage};
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::data_access::*};
use serde_json::json;
use sqlx::{PgConnection, Row, postgres::PgRow};

pub(crate) fn decode(row: &PgRow) -> Result<DataAccessGrant, BackendError> {
    Ok(DataAccessGrant {
        id: row.try_get("id").map_err(storage::error)?,
        tenant_id: row.try_get("tenant_id").map_err(storage::error)?,
        request_id: row.try_get("request_id").map_err(storage::error)?,
        request_version: row.try_get("request_version").map_err(storage::error)?,
        grantee_id: row.try_get("grantee_id").map_err(storage::error)?,
        target: serde_json::from_str(
            &row.try_get::<String, _>("target_snapshot")
                .map_err(storage::error)?,
        )
        .map_err(|_| storage::invalid("授权目标无效"))?,
        columns: serde_json::from_str(
            &row.try_get::<String, _>("columns_snapshot")
                .map_err(storage::error)?,
        )
        .map_err(|_| storage::invalid("授权列无效"))?,
        granted_by: row.try_get("granted_by").map_err(storage::error)?,
        granted_at: storage::time(row, "granted_at")?,
        expires_at: storage::time(row, "expires_at")?,
        revoked_at: storage::optional_time(row, "revoked_at")?,
        revoked_by: row.try_get("revoked_by").map_err(storage::error)?,
    })
}
pub(super) async fn get(
    conn: &mut PgConnection,
    tenant: &str,
    request: &str,
) -> Result<Option<DataAccessGrant>, BackendError> {
    sqlx::query("SELECT * FROM data_access_grant WHERE tenant_id=$1 AND request_id=$2 FOR SHARE")
        .bind(tenant)
        .bind(request)
        .fetch_optional(conn)
        .await
        .map_err(storage::error)?
        .as_ref()
        .map(decode)
        .transpose()
}
pub(super) async fn create(
    conn: &mut PgConnection,
    request: &DataAccessRequest,
    params: &DecideDataAccessParams,
    version: i64,
) -> Result<DataAccessGrant, BackendError> {
    let now = storage::now(conn).await?;
    let expires = params.expires_at.unwrap_or(request.expires_at);
    if expires <= now || expires > request.expires_at || expires > now + chrono::Duration::days(365)
    {
        return Err(storage::invalid("授权失效时间须在未来且不能超过申请期限"));
    }
    let active: Option<String> = sqlx::query_scalar(
        "SELECT id FROM \"user\" WHERE tenant_id=$1 AND id=$2 AND status='active' FOR SHARE",
    )
    .bind(&params.tenant_id)
    .bind(&request.requester_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage::error)?;
    if active.is_none() || !storage::verified(conn, &params.tenant_id, &request.target).await? {
        return Err(storage::conflict("被授权用户或元数据版本已经失效"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO data_access_grant(id,tenant_id,request_id,request_version,grantee_id,metadata_id,target_snapshot,columns_snapshot,granted_by,granted_at,expires_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(&id).bind(&params.tenant_id).bind(&params.request_id).bind(version).bind(&request.requester_id).bind(&request.target.metadata_id)
        .bind(json!(request.target).to_string()).bind(json!(request.requested_columns).to_string()).bind(&params.actor_id)
        .bind(now.to_rfc3339()).bind(expires.to_rfc3339()).execute(&mut *conn).await.map_err(storage::error)?;
    get(conn, &params.tenant_id, &params.request_id)
        .await?
        .ok_or_else(|| storage::conflict("授权未真实创建"))
}
pub(super) async fn revoke(
    conn: &mut PgConnection,
    params: &DecideDataAccessParams,
) -> Result<DataAccessGrant, BackendError> {
    let now = storage::now(conn).await?;
    let changed=sqlx::query("UPDATE data_access_grant SET revoked_at=$1,revoked_by=$2 WHERE tenant_id=$3 AND request_id=$4 AND revoked_at IS NULL")
        .bind(now.to_rfc3339()).bind(&params.actor_id).bind(&params.tenant_id).bind(&params.request_id)
        .execute(&mut *conn).await.map_err(storage::error)?.rows_affected();
    if changed != 1 {
        return Err(storage::conflict("授权不存在或已撤销"));
    }
    get(conn, &params.tenant_id, &params.request_id)
        .await?
        .ok_or_else(|| storage::conflict("授权不存在"))
}
pub(super) async fn active(
    repo: &PgDataAccessRepo,
    tenant: &str,
    actor: &str,
    at: DateTime<Utc>,
) -> Result<Vec<DataAccessGrant>, BackendError> {
    let mut tx = repo.begin(tenant, actor).await?;
    let now = std::cmp::max(at, storage::now(&mut tx).await?);
    let budget=sqlx::query("SELECT COUNT(*) AS total,COALESCE(SUM(octet_length(g.target_snapshot)+octet_length(g.columns_snapshot)),0) AS bytes
        FROM data_access_grant g JOIN product_resource r ON r.tenant_id=g.tenant_id AND r.id=g.request_id
        WHERE g.tenant_id=$1 AND g.grantee_id=$2 AND g.revoked_at IS NULL AND g.expires_at::timestamptz>$3::timestamptz
        AND r.kind='data_access_request' AND r.status='approved' AND r.version=g.request_version")
        .bind(tenant).bind(actor).bind(now.to_rfc3339()).fetch_one(&mut *tx).await.map_err(storage::error)?;
    if budget.try_get::<i64, _>("total").map_err(storage::error)? > 1000
        || budget.try_get::<i64, _>("bytes").map_err(storage::error)? > 8 * 1024 * 1024
    {
        return Err(storage::invalid(
            "活动授权数量或完整快照超过容量，请缩小授权范围",
        ));
    }
    let rows=sqlx::query("SELECT g.* FROM data_access_grant g JOIN product_resource r ON r.tenant_id=g.tenant_id AND r.id=g.request_id
        WHERE g.tenant_id=$1 AND g.grantee_id=$2 AND g.revoked_at IS NULL AND g.expires_at::timestamptz>$3::timestamptz
        AND r.kind='data_access_request' AND r.status='approved' AND r.version=g.request_version ORDER BY g.id LIMIT 1001 FOR SHARE OF g,r")
        .bind(tenant).bind(actor).bind(now.to_rfc3339()).fetch_all(&mut *tx).await.map_err(storage::error)?;
    if rows.len() > 1000 {
        return Err(storage::invalid("活动授权超过1000，请缩小授权范围"));
    }
    let mut data = Vec::new();
    let mut bytes = 0usize;
    for row in &rows {
        let grant = decode(row)?;
        validation::target(&grant.target, tenant, &grant.columns)?;
        if storage::verified(&mut tx, tenant, &grant.target).await? {
            bytes += json!(grant).to_string().len();
            if bytes > 8 * 1024 * 1024 {
                return Err(storage::invalid("活动授权快照超过8MiB，请缩小授权范围"));
            }
            data.push(grant);
        }
    }
    tx.commit().await.map_err(storage::error)?;
    Ok(data)
}
