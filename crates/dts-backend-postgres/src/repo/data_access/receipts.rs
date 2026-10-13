use super::storage;
use dts_backend::{BackendError, repo::data_access::*};
use serde_json::Value;
use sqlx::{PgConnection, Row};

pub(super) async fn replay(
    conn: &mut PgConnection,
    params: &DecideDataAccessParams,
    semantics: &Value,
) -> Result<Option<DataAccessDecisionResult>, BackendError> {
    let row=sqlx::query("SELECT actor_id,request_semantics,result_snapshot FROM data_access_decision WHERE tenant_id=$1 AND idempotency_key=$2")
        .bind(&params.tenant_id).bind(&params.idempotency_key).fetch_optional(conn).await.map_err(storage::error)?;
    let Some(row) = row else { return Ok(None) };
    let original: Value = serde_json::from_str(
        &row.try_get::<String, _>("request_semantics")
            .map_err(storage::error)?,
    )
    .map_err(|_| storage::invalid("审批回执参数无效"))?;
    if row
        .try_get::<String, _>("actor_id")
        .map_err(storage::error)?
        != params.actor_id
        || &original != semantics
    {
        return Err(storage::conflict("幂等键已用于另一操作或另一账号"));
    }
    let mut result: DataAccessDecisionResult = serde_json::from_str(
        &row.try_get::<String, _>("result_snapshot")
            .map_err(storage::error)?,
    )
    .map_err(|_| storage::invalid("审批回执结果无效"))?;
    result.replayed = true;
    Ok(Some(result))
}
pub(super) async fn save(
    conn: &mut PgConnection,
    params: &DecideDataAccessParams,
    semantics: &Value,
    result: &DataAccessDecisionResult,
) -> Result<(), BackendError> {
    let encoded =
        serde_json::to_string(result).map_err(|_| storage::invalid("审批回执无法编码"))?;
    if encoded.len() > 2 * 1024 * 1024 {
        return Err(storage::invalid("审批回执超过2MiB"));
    }
    let now = storage::now(conn).await?;
    sqlx::query("INSERT INTO data_access_decision(id,tenant_id,request_id,actor_id,idempotency_key,action,request_semantics,result_snapshot,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&result.decision_id).bind(&params.tenant_id).bind(&params.request_id).bind(&params.actor_id).bind(&params.idempotency_key)
        .bind(params.decision.as_str()).bind(semantics.to_string()).bind(encoded).bind(now.to_rfc3339())
        .execute(conn).await.map_err(storage::error)?;
    Ok(())
}
