use super::*;
#[test]
fn source_failure_is_unknown_not_zero() {
    let value = unavailable(
        "jobs",
        "product_job",
        "actor=current",
        BackendError::Query("private db detail".into()),
    );
    assert_eq!(value.state, ObservationState::Unknown);
    assert_eq!(value.total, None);
    let encoded = serde_json::to_string(&value).unwrap();
    assert!(!encoded.contains("private db detail"));
}
#[test]
fn denied_source_is_restricted_not_success() {
    let value = unavailable(
        "approvals",
        "data_access_request",
        "tenant=current",
        BackendError::Forbidden("private".into()),
    );
    assert_eq!(value.state, ObservationState::Restricted);
    assert_eq!(value.total, None);
}
#[test]
fn a_real_empty_source_is_known_zero() {
    let value = known(
        "inbox",
        "message_delivery",
        "recipient=current",
        Utc::now(),
        0,
    );
    assert_eq!(value.state, ObservationState::Known);
    assert_eq!(value.total, Some(0));
}
#[test]
fn source_and_run_semantics_remain_distinct() {
    assert!(
        statistics_provenance(StatisticsSource::SyncDefinitions)
            .1
            .contains("NOT executions")
    );
    assert!(
        statistics_provenance(StatisticsSource::ServiceCalls)
            .1
            .contains("pending reservation != succeeded")
    );
    assert!(
        statistics_provenance(StatisticsSource::LoginEvents)
            .1
            .contains("unassigned authentication failures excluded")
    );
}
