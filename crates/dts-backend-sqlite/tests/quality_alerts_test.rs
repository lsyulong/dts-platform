mod quality_alerts_support;
use dts_backend::{BackendError, repo::quality_alerts::*};
use quality_alerts_support::*;
use serde_json::json;

#[tokio::test]
async fn actual_open_acknowledgment_and_replay_are_atomic_and_notices_are_self_scoped() {
    let (pool, repo, params) = setup().await;
    let first = repo.record_evaluation(&params).await.unwrap();
    let alert = first.alert.unwrap();
    assert_eq!(alert.status, "open");
    assert_eq!(count(&pool, "quality_alert_event").await, 1);
    assert_eq!(count(&pool, "quality_alert_notice").await, 1);
    let replay = repo.record_evaluation(&params).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.alert.unwrap().id, alert.id);
    let notices = repo.list_notices("a", "reader", true, 10, 0).await.unwrap();
    assert_eq!(notices.total, 1);
    assert_eq!(notices.data[0].delivery_status, "pending");
    assert!(!notices.data[0].subject.contains("3"));
    assert!(
        repo.mark_notice_read("b", "other", &notices.data[0].id)
            .await
            .is_err()
    );
    let read = repo
        .mark_notice_read("a", "reader", &notices.data[0].id)
        .await
        .unwrap();
    let again = repo
        .mark_notice_read("a", "reader", &read.id)
        .await
        .unwrap();
    assert_eq!(read.read_at, again.read_at);
    let ack = AcknowledgeQualityAlertParams {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        alert_id: alert.id,
        expected_version: 1,
        idempotency_key: "ack-key".into(),
        reason: "Investigating upstream null values".into(),
    };
    assert_eq!(repo.acknowledge(&ack).await.unwrap().status, "acknowledged");
    assert_eq!(repo.acknowledge(&ack).await.unwrap().version, 2);
    assert_eq!(count(&pool, "quality_alert_event").await, 2);
    let mut conflict = ack;
    conflict.reason = "different request".into();
    assert!(matches!(
        repo.acknowledge(&conflict).await,
        Err(BackendError::Conflict(_))
    ));
    sqlx::query("UPDATE product_resource SET version=2 WHERE tenant_id='a' AND kind='quality_monitor' AND id='monitor'").execute(&pool).await.unwrap();
    assert!(
        repo.replay_evaluation("a", "actor", "monitor", 1, "evaluate-key")
            .await
            .unwrap()
            .unwrap()
            .replayed
    );
}

#[tokio::test]
async fn unavailable_observation_preserves_open_and_actual_recovery_resolves() {
    let (pool, repo, params) = setup().await;
    let alert = repo
        .record_evaluation(&params)
        .await
        .unwrap()
        .alert
        .unwrap();
    let mut unavailable = params;
    unavailable.idempotency_key = "unavailable".into();
    unavailable.observation.id = "unavailable-observation".into();
    unavailable.observation.summary["window_complete"] = json!(false);
    unavailable.observation.summary["threshold_exceeded"] = json!(null);
    unavailable.observation.state = "unavailable".into();
    unavailable.observation.reason = "incomplete_window".into();
    let current = repo
        .record_evaluation(&unavailable)
        .await
        .unwrap()
        .alert
        .unwrap();
    assert_eq!(current.status, "open");
    assert_eq!(current.version, alert.version + 1);
    assert_eq!(current.last_observation_id, "unavailable-observation");
    let mut recover = unavailable;
    recover.idempotency_key = "recovery".into();
    recover.observation.id = "recovered-observation".into();
    recover.observation.summary = summary(0);
    recover.observation.state = "within_threshold".into();
    recover.observation.reason = "within_threshold".into();
    recover.observation.snapshot.jobs[0].nodes[0]
        .result
        .as_mut()
        .unwrap()["failed_rows"] = json!(0);
    // Recovery uses a new durable run in a later non-overlapping window.
    // The first observation and its actual failure counts remain unchanged.
    recover.observation.period_end += chrono::Duration::hours(2);
    recover.observation.period_start += chrono::Duration::hours(2);
    recover.observation.created_at = recover.observation.period_end;
    let event = recover.observation.period_end - chrono::Duration::seconds(1);
    sqlx::query("INSERT INTO product_job(id,tenant_id,source_kind,source_id,source_version,actor_id,action,idempotency_key,definition_snapshot,parameters,status,can_retry,created_at,updated_at,started_at,finished_at) SELECT 'recovery-job',tenant_id,source_kind,source_id,source_version,actor_id,action,'recovery-job-key',definition_snapshot,parameters,'succeeded',can_retry,?1,?1,?1,?1 FROM product_job WHERE id='job'").bind(event.to_rfc3339()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO product_job_node(tenant_id,job_id,node_id,name,definition,depends_on,status,result,started_at,finished_at) SELECT tenant_id,'recovery-job',node_id,name,definition,depends_on,'succeeded',?1,?2,?2 FROM product_job_node WHERE job_id='job'").bind(recover.observation.snapshot.jobs[0].nodes[0].result.as_ref().unwrap().to_string()).bind(event.to_rfc3339()).execute(&pool).await.unwrap();
    recover.observation.snapshot.jobs[0].id = "recovery-job".into();
    recover.observation.snapshot.jobs[0].updated_at = event;
    recover.observation.snapshot.jobs[0].event_time = event;
    let resolved = repo
        .record_evaluation(&recover)
        .await
        .unwrap()
        .alert
        .unwrap();
    assert_eq!(resolved.status, "resolved");
    assert!(resolved.resolved_at.is_some());
    assert_eq!(count(&pool, "quality_alert_notice").await, 1);
}

#[tokio::test]
async fn altered_proofs_fake_pass_cross_tenant_recipients_and_permission_loss_rollback() {
    let (pool, repo, params) = setup().await;
    let mut fake = RecordQualityEvaluationParams {
        observation: params.observation.clone(),
        idempotency_key: "fake".into(),
        recipient_ids: vec![],
        notice_subject: "Quality".into(),
    };
    fake.observation.summary["failed_rows"] = json!(0);
    fake.observation.summary["failure_ratio"] = json!(0.0);
    fake.observation.summary["threshold_exceeded"] = json!(false);
    fake.observation.state = "within_threshold".into();
    assert!(repo.record_evaluation(&fake).await.is_err());
    let mut invalid = fake;
    invalid.observation = params.observation.clone();
    invalid.recipient_ids = vec!["other".into()];
    assert!(repo.record_evaluation(&invalid).await.is_err());
    assert_eq!(count(&pool, "quality_observation").await, 0);
    sqlx::query(
        "UPDATE product_resource SET status='draft' WHERE tenant_id='a' AND kind='metadata_object'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(repo.record_evaluation(&params).await.is_err());
    assert_eq!(count(&pool, "quality_observation").await, 0);
    sqlx::query("UPDATE product_resource SET status='published' WHERE tenant_id='a' AND kind='metadata_object'")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE metadata_catalog SET discovery_status='missing' WHERE tenant_id='a'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.record_evaluation(&params).await.is_err());
    assert_eq!(count(&pool, "quality_alert").await, 0);
    sqlx::query("UPDATE metadata_catalog SET discovery_status='present' WHERE tenant_id='a'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM role_permission WHERE role_id='role-a' AND permission='quality:write'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        repo.record_evaluation(&params).await,
        Err(BackendError::Forbidden(_))
    ));
    assert_eq!(count(&pool, "quality_alert_event").await, 0);
}
