use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::quality_alerts::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgConnection, Row};

pub(super) fn error(value: sqlx::Error) -> BackendError {
    if value
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "40001" || code == "40P01")
    {
        return conflict("评估期间真实数据状态发生并发变化，请使用原请求标识重试");
    }
    super::super::product_resource::helpers::map_error(value)
}
pub(super) fn conflict(message: &str) -> BackendError {
    BackendError::Conflict(message.into())
}
pub(super) fn encode(value: &impl Serialize) -> Result<String, BackendError> {
    serde_json::to_string(value).map_err(|_| BackendError::Internal("质量告警编码失败".into()))
}
pub(super) fn decode<T: DeserializeOwned>(value: &str) -> Result<T, BackendError> {
    serde_json::from_str(value).map_err(|_| BackendError::Internal("持久化质量告警格式无效".into()))
}
pub(super) async fn active(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
) -> Result<(), BackendError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.tenant_id=$1 AND u.id=$2 AND u.status='active' AND t.status='active')").bind(tenant).bind(actor).fetch_one(&mut *conn).await.map_err(error)?;
    if !active {
        return Err(BackendError::Forbidden("质量告警用户或租户已停用".into()));
    }
    Ok(())
}
pub(super) async fn authority(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    write: bool,
) -> Result<(), BackendError> {
    let active:Option<String>=sqlx::query_scalar("SELECT u.id FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE u.tenant_id=$1 AND u.id=$2 AND u.status='active' AND t.status='active' FOR SHARE OF u,t").bind(tenant).bind(actor).fetch_optional(&mut *conn).await.map_err(error)?;
    if active.is_none() {
        return Err(BackendError::Forbidden("质量告警用户或租户已停用".into()));
    }
    let required = if write {
        "quality:write"
    } else {
        "quality:read"
    };
    let granted:Vec<String>=sqlx::query_scalar("SELECT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2 AND rp.permission=$3 FOR SHARE OF ur,r,rp").bind(actor).bind(tenant).bind(required).fetch_all(conn).await.map_err(error)?;
    if granted.is_empty() {
        return Err(BackendError::Forbidden("缺少当前质量模块权限".into()));
    }
    Ok(())
}
pub(super) async fn receipt<T: DeserializeOwned>(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    key: &str,
    semantics: &Value,
) -> Result<Option<T>, BackendError> {
    let row=sqlx::query("SELECT semantics,result FROM quality_alert_receipt WHERE tenant_id=$1 AND actor_id=$2 AND idempotency_key=$3").bind(tenant).bind(actor).bind(key).fetch_optional(conn).await.map_err(error)?;
    let Some(row) = row else { return Ok(None) };
    if decode::<Value>(&row.try_get::<String, _>("semantics").map_err(error)?)? != *semantics {
        return Err(conflict("该幂等请求标识已绑定不同质量动作"));
    }
    Ok(Some(decode(
        &row.try_get::<String, _>("result").map_err(error)?,
    )?))
}
pub(super) async fn save_receipt(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
    key: &str,
    semantics: &Value,
    result: &impl Serialize,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO quality_alert_receipt(tenant_id,actor_id,idempotency_key,semantics,result) VALUES($1,$2,$3,$4,$5)").bind(tenant).bind(actor).bind(key).bind(encode(semantics)?).bind(encode(result)?).execute(conn).await.map_err(error)?;
    Ok(())
}
pub(super) async fn observation(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<QualityObservation>, BackendError> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT payload FROM quality_observation WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(id)
            .fetch_optional(conn)
            .await
            .map_err(error)?;
    value.map(|value| decode(&value)).transpose()
}
pub(super) async fn alert(
    conn: &mut PgConnection,
    tenant: &str,
    id: &str,
) -> Result<Option<QualityAlert>, BackendError> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT payload FROM quality_alert WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(id)
            .fetch_optional(conn)
            .await
            .map_err(error)?;
    value.map(|value| decode(&value)).transpose()
}
pub(super) async fn save_alert(
    conn: &mut PgConnection,
    alert: &QualityAlert,
    previous: Option<i64>,
) -> Result<(), BackendError> {
    if let Some(version) = previous {
        let changed=sqlx::query("UPDATE quality_alert SET status=$1,version=$2,last_observation_id=$3,payload=$4,updated_at=$5 WHERE tenant_id=$6 AND id=$7 AND version=$8").bind(&alert.status).bind(alert.version).bind(&alert.last_observation_id).bind(encode(alert)?).bind(alert.updated_at).bind(&alert.tenant_id).bind(&alert.id).bind(version).execute(conn).await.map_err(error)?.rows_affected();
        if changed != 1 {
            return Err(conflict("质量告警版本已改变"));
        }
    } else {
        sqlx::query("INSERT INTO quality_alert(id,tenant_id,monitor_id,status,version,opening_observation_id,last_observation_id,payload,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(&alert.id).bind(&alert.tenant_id).bind(&alert.monitor_id).bind(&alert.status).bind(alert.version).bind(&alert.opening_observation_id).bind(&alert.last_observation_id).bind(encode(alert)?).bind(alert.updated_at).execute(conn).await.map_err(error)?;
    }
    Ok(())
}
pub(super) async fn event(
    conn: &mut PgConnection,
    event: &QualityAlertEvent,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO quality_alert_event(id,tenant_id,alert_id,observation_id,actor_id,payload,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&event.id).bind(&event.tenant_id).bind(&event.alert_id).bind(&event.observation_id).bind(&event.actor_id).bind(encode(event)?).bind(event.created_at).execute(conn).await.map_err(error)?;
    Ok(())
}
pub(super) fn now() -> DateTime<Utc> {
    Utc::now()
}
