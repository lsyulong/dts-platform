use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::message_delivery::*};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row, postgres::PgRow};

pub(super) use crate::repo::product_resource::map_reference_error as map_error;

pub(super) fn decode(row: &PgRow) -> Result<MessageDelivery, BackendError> {
    let delivered_at: DateTime<Utc> = row.try_get("delivered_at").map_err(map_error)?;
    let read_at: Option<DateTime<Utc>> = row.try_get("read_at").map_err(map_error)?;
    Ok(MessageDelivery {
        id: row.try_get("id").map_err(map_error)?,
        tenant_id: row.try_get("tenant_id").map_err(map_error)?,
        source_kind: row.try_get("source_kind").map_err(map_error)?,
        message_id: row.try_get("message_id").map_err(map_error)?,
        message_version: row.try_get("message_version").map_err(map_error)?,
        recipient_id: row.try_get("recipient_id").map_err(map_error)?,
        subject: row.try_get("subject").map_err(map_error)?,
        body: row.try_get("body").map_err(map_error)?,
        status: if read_at.is_some() {
            "read"
        } else {
            "delivered"
        }
        .into(),
        delivered_at,
        read_at,
    })
}

pub(super) async fn validate_source(
    conn: &mut PgConnection,
    params: &DeliverInAppParams,
    request: &Value,
) -> Result<(), BackendError> {
    let row = sqlx::query("SELECT status,version,properties::text AS properties FROM product_resource WHERE tenant_id=$1 AND kind='system_message' AND id=$2 FOR UPDATE")
        .bind(&params.tenant_id).bind(&params.message_id).fetch_optional(&mut *conn).await.map_err(map_error)?
        .ok_or_else(|| BackendError::NotFound("消息不存在或不属于当前租户".into()))?;
    let version: i64 = row.try_get("version").map_err(map_error)?;
    let status: String = row.try_get("status").map_err(map_error)?;
    if version != params.expected_version || status != "draft" {
        return Err(BackendError::Conflict(
            "只有当前版本的草稿消息可以投递".into(),
        ));
    }
    let properties: String = row.try_get("properties").map_err(map_error)?;
    let properties: Value = serde_json::from_str(&properties)
        .map_err(|_| BackendError::Internal("消息属性格式无效".into()))?;
    if properties["channel"] != "in_app" {
        return Err(BackendError::Configuration(
            "当前渠道没有配置实际投递驱动".into(),
        ));
    }
    let mut recipients: Vec<String> = properties["recipient_ids"]
        .as_array()
        .ok_or_else(|| BackendError::ConstraintViolation("消息收件人格式无效".into()))?
        .iter()
        .map(|id| {
            id.as_str()
                .map(str::to_owned)
                .ok_or_else(|| BackendError::ConstraintViolation("消息收件人格式无效".into()))
        })
        .collect::<Result<_, _>>()?;
    recipients.sort();
    if json!(recipients) != request["recipient_ids"] {
        return Err(BackendError::Conflict("投递收件人与消息定义不一致".into()));
    }
    for id in params
        .recipient_ids
        .iter()
        .chain(std::iter::once(&params.actor_id))
    {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM \"user\" WHERE tenant_id=$1 AND id=$2 AND status='active' FOR SHARE",
        )
        .bind(&params.tenant_id)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_error)?;
        if exists.is_none() {
            return Err(BackendError::ConstraintViolation(
                "投递用户不存在、已停用或不属于当前租户".into(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn audit_send(
    conn: &mut PgConnection,
    params: &DeliverInAppParams,
    now: &DateTime<Utc>,
) -> Result<(), BackendError> {
    let summary = json!({"action":"send", "channel":"in_app", "recipient_count":params.recipient_ids.len(), "before_version":params.expected_version,"version":params.expected_version+1});
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES($1,$2,$3,'system_message',$4,'update',$5,$6)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&params.tenant_id).bind(&params.actor_id).bind(&params.message_id)
        .bind(&summary).bind(now).execute(conn).await.map_err(map_error)?;
    Ok(())
}

pub(super) async fn audit_read(
    conn: &mut PgConnection,
    delivery: &MessageDelivery,
    now: &DateTime<Utc>,
) -> Result<(), BackendError> {
    let summary =
        json!({"action":"mark_read", "delivery_id":delivery.id,"version":delivery.message_version});
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES($1,$2,$3,$4,$5,'update',$6,$7)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&delivery.tenant_id).bind(&delivery.recipient_id).bind(&delivery.source_kind).bind(&delivery.message_id)
        .bind(&summary).bind(now).execute(conn).await.map_err(map_error)?;
    Ok(())
}
