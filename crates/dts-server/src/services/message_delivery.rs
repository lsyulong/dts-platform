use std::{collections::HashSet, sync::Arc};

use dts_backend::{ProductResourceRepo, UserRepo, repo::message_delivery::*};
use serde_json::{Value, json};

use crate::{models::error::AppError, services::product_domain::access};

#[derive(Clone)]
pub struct MessageService {
    resources: Arc<dyn ProductResourceRepo>,
    users: Arc<dyn UserRepo>,
    deliveries: Arc<dyn MessageDeliveryRepo>,
}

impl MessageService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        users: Arc<dyn UserRepo>,
        deliveries: Arc<dyn MessageDeliveryRepo>,
    ) -> Self {
        Self {
            resources,
            users,
            deliveries,
        }
    }

    pub async fn send(
        &self,
        tenant_id: &str,
        message_id: &str,
        expected_version: i64,
        actor_id: &str,
        idempotency_key: &str,
    ) -> Result<MessageDeliveryBatch, AppError> {
        if idempotency_key.is_empty()
            || idempotency_key.len() > 128
            || idempotency_key != idempotency_key.trim()
            || idempotency_key.chars().any(char::is_control)
        {
            return Err(AppError::BadRequest(
                "请提供 1 至 128 字符的有效幂等请求标识".into(),
            ));
        }
        let message = self
            .resources
            .get(tenant_id, "system_message", message_id)
            .await?
            .ok_or_else(|| AppError::NotFound("消息不存在".into()))?;
        if let Some(batch) = self
            .deliveries
            .get_batch(
                tenant_id,
                message_id,
                expected_version,
                actor_id,
                idempotency_key,
            )
            .await?
        {
            return Ok(batch);
        }
        if message.version != expected_version || message.status != "draft" {
            return Err(AppError::Conflict(
                "只有当前版本草稿可以发送；已投递消息须使用原请求标识重放".into(),
            ));
        }
        access::validate("system_message", &message.properties)?;
        if message.properties["channel"] != "in_app" {
            return Err(AppError::BadRequest(
                "此消息渠道未配置真实投递驱动；当前仅支持站内消息".into(),
            ));
        }
        let recipients = message.properties["recipient_ids"]
            .as_array()
            .ok_or_else(|| AppError::BadRequest("消息收件人格式无效".into()))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| AppError::BadRequest("消息收件人格式无效".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let users = self
            .users
            .list_all(Some(tenant_id), None, Some("active"))
            .await?;
        let active: HashSet<_> = users
            .iter()
            .filter(|user| user.tenant_id == tenant_id && user.status == "active")
            .map(|user| user.id.as_str())
            .collect();
        if !active.contains(actor_id) || recipients.iter().any(|id| !active.contains(id.as_str())) {
            return Err(AppError::BadRequest(
                "发送人或收件人不存在、已停用或不属于当前租户".into(),
            ));
        }
        let parameters = message
            .properties
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let rendered = if let Some(template_id) = message
            .properties
            .get("template_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            let template = self
                .resources
                .get(tenant_id, "message_template", template_id)
                .await?
                .ok_or_else(|| AppError::BadRequest("消息模板不存在或不属于当前租户".into()))?;
            if template.properties["channel"] != message.properties["channel"] {
                return Err(AppError::BadRequest("消息模板渠道与消息渠道不一致".into()));
            }
            access::render_template(&template.properties, &parameters)?
        } else {
            if parameters
                .as_object()
                .is_none_or(|values| !values.is_empty())
            {
                return Err(AppError::BadRequest("未选择模板时不能提交模板参数".into()));
            }
            access::RenderedMessage {
                subject: message.name,
                body: message.properties["content"]
                    .as_str()
                    .ok_or_else(|| AppError::BadRequest("消息内容缺失".into()))?
                    .to_owned(),
            }
        };
        Ok(self
            .deliveries
            .deliver_in_app(&DeliverInAppParams {
                tenant_id: tenant_id.into(),
                message_id: message_id.into(),
                expected_version,
                actor_id: actor_id.into(),
                idempotency_key: idempotency_key.into(),
                subject: rendered.subject,
                body: rendered.body,
                recipient_ids: recipients,
            })
            .await?)
    }

    pub async fn inbox(
        &self,
        tenant_id: &str,
        authenticated_recipient_id: &str,
        unread_only: bool,
        limit: u32,
        offset: u32,
    ) -> Result<MessageInboxPage, AppError> {
        Ok(self
            .deliveries
            .list_inbox(&MessageInboxFilter {
                tenant_id: tenant_id.into(),
                recipient_id: authenticated_recipient_id.into(),
                unread_only,
                limit,
                offset,
            })
            .await?)
    }

    pub async fn mark_read(
        &self,
        tenant_id: &str,
        authenticated_recipient_id: &str,
        delivery_id: &str,
    ) -> Result<MessageDelivery, AppError> {
        Ok(self
            .deliveries
            .mark_read(tenant_id, authenticated_recipient_id, delivery_id)
            .await?)
    }
}
