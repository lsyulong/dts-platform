use dts_backend::BackendError;
use serde_json::Value;
use sqlx::{PgConnection, Row};

use super::storage::{self, conflict, error, invalid};

pub(super) struct ReceiptScope<'a> {
    pub tenant: &'a str,
    pub application: &'a str,
    pub actor: &'a str,
    pub action: &'a str,
    pub key: &'a str,
}

pub(super) async fn credential(
    conn: &mut PgConnection,
    scope: &ReceiptScope<'_>,
    semantic: &Value,
) -> Result<Option<Value>, BackendError> {
    let row=sqlx::query("SELECT actor_id,action,semantics,result FROM service_credential_receipt WHERE tenant_id=$1 AND application_id=$2 AND idempotency_key=$3")
        .bind(scope.tenant).bind(scope.application).bind(scope.key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    let existing: Value =
        serde_json::from_str(&row.try_get::<String, _>("semantics").map_err(error)?)
            .map_err(|_| invalid("服务凭据幂等语义无效"))?;
    if existing != *semantic
        || row.try_get::<String, _>("actor_id").map_err(error)? != scope.actor
        || row.try_get::<String, _>("action").map_err(error)? != scope.action
    {
        return Err(conflict("同幂等键不能操作不同用户或不同凭据语义"));
    }
    serde_json::from_str(&row.try_get::<String, _>("result").map_err(error)?)
        .map(Some)
        .map_err(|_| invalid("服务凭据回执无效"))
}

pub(super) async fn save_credential(
    conn: &mut PgConnection,
    scope: &ReceiptScope<'_>,
    semantic: &Value,
    result: &Value,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO service_credential_receipt(tenant_id,application_id,idempotency_key,actor_id,action,semantics,result,created_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(scope.tenant).bind(scope.application).bind(scope.key).bind(scope.actor).bind(scope.action).bind(semantic.to_string()).bind(result.to_string())
        .bind(storage::now(conn).await?).execute(conn).await.map_err(error)?;
    Ok(())
}
