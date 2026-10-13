use chrono::{DateTime, Utc};
use dts_backend::repo::quality_alerts::QualityAlert;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityEvaluateRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityAckRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct QualityObservationView {
    pub id: String,
    pub monitor_id: String,
    pub monitor_version: i64,
    pub object_id: String,
    pub actor_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub state: String,
    pub reason: String,
    pub summary: Value,
    pub created_at: DateTime<Utc>,
    pub result_access: String,
    pub dependencies: Vec<QualityDependencyView>,
    pub run_ids: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct QualityDependencyView {
    pub kind: String,
    pub id: String,
    pub version: i64,
}
#[derive(Debug, Serialize)]
pub struct QualityAlertView {
    pub alert: QualityAlert,
    pub observation: QualityObservationView,
}
#[derive(Debug, Serialize)]
pub struct QualityEvaluationView {
    pub observation: QualityObservationView,
    pub alert: Option<QualityAlert>,
    pub replayed: bool,
}
pub type QualityObservationPageView =
    dts_backend::repo::quality_alerts::QualityPage<QualityObservationView>;
pub type QualityAlertPageView = dts_backend::repo::quality_alerts::QualityPage<QualityAlertView>;
pub type QualityAlertEventPage = dts_backend::repo::quality_alerts::QualityPage<
    dts_backend::repo::quality_alerts::QualityAlertEvent,
>;
