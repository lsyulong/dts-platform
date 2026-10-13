// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertPolicyDelivery {
    pub id: String,
    pub recipient_id: String,
    pub subject: String,
    pub body: String,
    pub channel: String,
    pub status: String,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertPolicyEvaluation {
    pub id: String,
    pub tenant_id: String,
    pub policy_id: String,
    pub policy_version: i64,
    pub event_type: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub observed_count: i64,
    pub threshold: i64,
    pub state: String,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub actor_id: String,
    pub created_at: DateTime<Utc>,
    pub deliveries: Vec<AlertPolicyDelivery>,
    pub replayed: bool,
}

#[derive(Debug, Serialize)]
pub struct AlertPolicyEvaluationPage {
    pub data: Vec<AlertPolicyEvaluation>,
    pub total: i64,
}

pub struct EvaluateAlertPolicyParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub policy_id: String,
    pub policy_version: i64,
    pub policy_name: String,
    pub event_type: String,
    pub window_seconds: i64,
    pub threshold: i64,
    pub cooldown_seconds: i64,
    pub recipient_ids: Vec<String>,
    pub subject: String,
    pub body: String,
    pub channel: String,
    pub idempotency_key: String,
    pub test_delivery: bool,
}

pub struct AlertPolicyEvaluationFilter {
    pub tenant_id: String,
    pub policy_id: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertDeliveryClaim {
    pub id: String,
    pub tenant_id: String,
    pub evaluation_id: String,
    pub recipient_id: String,
    pub subject: String,
    pub body: String,
    pub channel: String,
    pub attempt_count: i64,
    pub lease_owner: String,
    pub lease_fence: i64,
}

pub struct ClaimAlertDeliveryParams {
    pub worker_id: String,
    pub lease_seconds: u32,
    pub max_attempts: u32,
}

#[derive(Debug, Clone)]
pub struct AlertDeliveryLease {
    pub worker_id: String,
    pub fence: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum AlertDeliveryOutcome {
    Delivered,
    Retry,
    Failed,
}

pub struct FinishAlertDeliveryParams {
    pub tenant_id: String,
    pub delivery_id: String,
    pub lease: AlertDeliveryLease,
    pub outcome: AlertDeliveryOutcome,
    pub error: Option<String>,
    pub retry_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ScheduledAlertPolicy {
    pub tenant_id: String,
    pub policy_id: String,
    pub policy_version: i64,
    pub actor_id: String,
    pub scheduled_for: DateTime<Utc>,
    pub lease_owner: String,
    pub lease_fence: i64,
}

pub struct ClaimScheduledAlertPolicyParams {
    pub worker_id: String,
    pub lease_seconds: u32,
}

pub struct FinishScheduledAlertPolicyParams {
    pub tenant_id: String,
    pub policy_id: String,
    pub lease: AlertDeliveryLease,
    pub next_evaluation_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[async_trait]
pub trait AlertPolicyRepo: Send + Sync {
    async fn evaluate(
        &self,
        params: &EvaluateAlertPolicyParams,
    ) -> Result<AlertPolicyEvaluation, BackendError>;
    async fn list(
        &self,
        filter: &AlertPolicyEvaluationFilter,
    ) -> Result<AlertPolicyEvaluationPage, BackendError>;
    async fn sync_schedules(&self) -> Result<u64, BackendError>;
    async fn claim_scheduled(
        &self,
        params: &ClaimScheduledAlertPolicyParams,
    ) -> Result<Option<ScheduledAlertPolicy>, BackendError>;
    async fn finish_scheduled(
        &self,
        params: &FinishScheduledAlertPolicyParams,
    ) -> Result<(), BackendError>;
    async fn claim_delivery(
        &self,
        params: &ClaimAlertDeliveryParams,
    ) -> Result<Option<AlertDeliveryClaim>, BackendError>;
    async fn finish_delivery(
        &self,
        params: &FinishAlertDeliveryParams,
    ) -> Result<AlertPolicyDelivery, BackendError>;
}

pub fn evaluation_hash(params: &EvaluateAlertPolicyParams) -> Result<String, BackendError> {
    if params.policy_version < 1
        || !matches!(
            params.event_type.as_str(),
            "task_failure" | "quality_failure" | "system_health"
        )
        || !(60..=604_800).contains(&params.window_seconds)
        || !(1..=1_000_000).contains(&params.threshold)
        || !(0..=604_800).contains(&params.cooldown_seconds)
        || params.recipient_ids.is_empty()
        || params.recipient_ids.len() > 1000
        || params.subject.trim().is_empty()
        || params.subject.chars().count() > 256
        || params.body.trim().is_empty()
        || params.body.len() > 65_536
        || !matches!(
            params.channel.as_str(),
            "in_app" | "email" | "sms" | "wechat_work" | "dingtalk" | "webhook"
        )
    {
        return Err(BackendError::ConstraintViolation(
            "告警策略执行参数无效".into(),
        ));
    }
    for value in [
        &params.tenant_id,
        &params.actor_id,
        &params.policy_id,
        &params.idempotency_key,
    ] {
        if value.is_empty()
            || value.len() > 128
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(BackendError::ConstraintViolation(
                "告警策略执行标识无效".into(),
            ));
        }
    }
    let mut recipients = params.recipient_ids.clone();
    recipients.sort();
    if recipients.windows(2).any(|values| values[0] == values[1])
        || recipients.iter().any(|value| {
            value.is_empty()
                || value.len() > 128
                || value.trim() != value
                || value.chars().any(char::is_control)
        })
    {
        return Err(BackendError::ConstraintViolation(
            "告警收件人无效或重复".into(),
        ));
    }
    let semantics = serde_json::to_vec(&serde_json::json!({
        "actor_id":params.actor_id,"policy_id":params.policy_id,"policy_version":params.policy_version,
        "event_type":params.event_type,"window_seconds":params.window_seconds,"threshold":params.threshold,
        "cooldown_seconds":params.cooldown_seconds,"recipient_ids":recipients,"subject":params.subject,
        "body":params.body,"test_delivery":params.test_delivery
        ,"channel":params.channel
    }))
    .map_err(|error| BackendError::Internal(format!("告警请求序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(semantics)))
}
