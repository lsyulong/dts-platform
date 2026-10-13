use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::BackendError;

pub mod validation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityResourceProof {
    pub kind: String,
    pub id: String,
    pub version: i64,
    pub properties: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityNodeProof {
    pub id: String,
    pub status: String,
    pub definition: Value,
    pub result: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityJobProof {
    pub id: String,
    pub source_id: String,
    pub source_version: i64,
    pub status: String,
    pub updated_at: DateTime<Utc>,
    pub event_time: DateTime<Utc>,
    pub definition_snapshot: Value,
    pub nodes: Vec<QualityNodeProof>,
}

/// Internal evidence. API views must never serialize this snapshot directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityObservationSnapshot {
    pub resources: Vec<QualityResourceProof>,
    pub metadata_id: String,
    pub metadata_version: i64,
    pub source_fingerprint: String,
    pub jobs: Vec<QualityJobProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityObservation {
    pub id: String,
    pub tenant_id: String,
    pub monitor_id: String,
    pub monitor_version: i64,
    pub object_id: String,
    pub actor_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub state: String,
    pub reason: String,
    pub summary: Value,
    pub snapshot: QualityObservationSnapshot,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAlert {
    pub id: String,
    pub tenant_id: String,
    pub monitor_id: String,
    pub monitor_version: i64,
    pub status: String,
    pub version: i64,
    pub opening_observation_id: String,
    pub last_observation_id: String,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAlertEvent {
    pub id: String,
    pub tenant_id: String,
    pub alert_id: String,
    pub observation_id: String,
    pub actor_id: String,
    pub action: String,
    pub reason: String,
    pub before_status: Option<String>,
    pub after_status: String,
    pub created_at: DateTime<Utc>,
}

/// A genuine outbox entry is not a claim of successful message delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAlertNotice {
    pub id: String,
    pub tenant_id: String,
    pub alert_id: String,
    pub recipient_id: String,
    pub subject: String,
    pub link: String,
    pub delivery_status: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct QualityPage<T> {
    pub data: Vec<T>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityEvaluation {
    pub observation: QualityObservation,
    pub alert: Option<QualityAlert>,
    pub replayed: bool,
}

pub struct RecordQualityEvaluationParams {
    pub observation: QualityObservation,
    pub idempotency_key: String,
    pub recipient_ids: Vec<String>,
    pub notice_subject: String,
}

pub struct AcknowledgeQualityAlertParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub alert_id: String,
    pub expected_version: i64,
    pub idempotency_key: String,
    pub reason: String,
}

pub struct QualityAlertFilter {
    pub tenant_id: String,
    pub monitor_id: String,
    pub status: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[async_trait]
pub trait QualityAlertRepo: Send + Sync {
    async fn replay_evaluation(
        &self,
        tenant: &str,
        actor: &str,
        monitor: &str,
        version: i64,
        key: &str,
    ) -> Result<Option<QualityEvaluation>, BackendError>;
    async fn record_evaluation(
        &self,
        params: &RecordQualityEvaluationParams,
    ) -> Result<QualityEvaluation, BackendError>;
    async fn get_observation(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<Option<QualityObservation>, BackendError>;
    async fn list_observations(
        &self,
        filter: &QualityAlertFilter,
    ) -> Result<QualityPage<QualityObservation>, BackendError>;
    async fn get_alert(&self, tenant: &str, id: &str)
    -> Result<Option<QualityAlert>, BackendError>;
    async fn list_alerts(
        &self,
        filter: &QualityAlertFilter,
    ) -> Result<QualityPage<QualityAlert>, BackendError>;
    async fn acknowledge(
        &self,
        params: &AcknowledgeQualityAlertParams,
    ) -> Result<QualityAlert, BackendError>;
    async fn list_events(
        &self,
        tenant: &str,
        alert: &str,
        limit: u32,
        offset: u32,
    ) -> Result<QualityPage<QualityAlertEvent>, BackendError>;
    async fn list_notices(
        &self,
        tenant: &str,
        recipient: &str,
        unread_only: bool,
        limit: u32,
        offset: u32,
    ) -> Result<QualityPage<QualityAlertNotice>, BackendError>;
    async fn mark_notice_read(
        &self,
        tenant: &str,
        recipient: &str,
        id: &str,
    ) -> Result<QualityAlertNotice, BackendError>;
}
