// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelivery {
    pub id: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub message_id: String,
    pub message_version: i64,
    pub recipient_id: String,
    pub subject: String,
    pub body: String,
    pub status: String,
    pub delivered_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

pub struct DeliverInAppParams {
    pub tenant_id: String,
    pub message_id: String,
    pub expected_version: i64,
    pub actor_id: String,
    pub idempotency_key: String,
    pub subject: String,
    pub body: String,
    pub recipient_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageDeliveryBatch {
    pub data: Vec<MessageDelivery>,
    pub resource_version: i64,
    pub replayed: bool,
}

pub struct MessageInboxFilter {
    pub tenant_id: String,
    pub recipient_id: String,
    pub unread_only: bool,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct MessageInboxPage {
    pub data: Vec<MessageDelivery>,
    pub total: i64,
}

/// Stores actual in-app delivery and marks the source sent in the same transaction.
/// Authorization belongs to the service. Inbox identities must come from session claims.
#[async_trait]
pub trait MessageDeliveryRepo: Send + Sync {
    async fn get_batch(
        &self,
        tenant_id: &str,
        message_id: &str,
        expected_version: i64,
        actor_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<MessageDeliveryBatch>, BackendError>;
    async fn deliver_in_app(
        &self,
        params: &DeliverInAppParams,
    ) -> Result<MessageDeliveryBatch, BackendError>;
    async fn list_inbox(
        &self,
        filter: &MessageInboxFilter,
    ) -> Result<MessageInboxPage, BackendError>;
    async fn mark_read(
        &self,
        tenant_id: &str,
        recipient_id: &str,
        delivery_id: &str,
    ) -> Result<MessageDelivery, BackendError>;
}

pub fn delivery_request(params: &DeliverInAppParams) -> Result<Value, BackendError> {
    let invalid = |message: &str| BackendError::ConstraintViolation(message.into());
    for value in [
        &params.tenant_id,
        &params.message_id,
        &params.actor_id,
        &params.idempotency_key,
    ] {
        if value.is_empty()
            || value.len() > 128
            || value.chars().any(char::is_control)
            || value != value.trim()
        {
            return Err(invalid("消息及幂等标识必须是 1 至 128 字符的有效标识"));
        }
    }
    if params.expected_version < 1 || params.expected_version == i64::MAX {
        return Err(invalid("消息版本无效"));
    }
    if params.subject.trim().is_empty()
        || params.subject.chars().count() > 256
        || params.subject.chars().any(char::is_control)
        || params.body.trim().is_empty()
        || params.body.len() > 65_536
        || params.body.contains('\0')
    {
        return Err(invalid("消息标题或内容无效或超过长度限制"));
    }
    if params.recipient_ids.is_empty() || params.recipient_ids.len() > 1000 {
        return Err(invalid("每批必须包含 1 至 1000 个真实收件人"));
    }
    let mut recipients = params.recipient_ids.clone();
    recipients.sort();
    if recipients.windows(2).any(|pair| pair[0] == pair[1])
        || recipients.iter().any(|id| {
            id.is_empty() || id.len() > 128 || id != id.trim() || id.chars().any(char::is_control)
        })
    {
        return Err(invalid("收件人标识无效或重复"));
    }
    Ok(json!({
        "message_id":params.message_id, "expected_version":params.expected_version,
        "actor_id":params.actor_id, "subject":params.subject, "body":params.body,
        "recipient_ids":recipients
    }))
}
