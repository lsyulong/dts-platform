// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::Utc;
use dts_backend::{
    BackendError,
    repo::{alert_policy::*, message_delivery::MessageDeliveryRepo, product_resource::*},
};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{
    SqliteAlertPolicyRepo, SqliteMessageDeliveryRepo, SqliteMigrationRunner,
    SqliteProductResourceRepo,
};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

async fn setup() -> (SqlitePool, SqliteAlertPolicyRepo) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other')").execute(&pool).await.unwrap();
    for (id, tenant) in [
        ("actor", "tenant"),
        ("recipient", "tenant"),
        ("foreign", "other"),
    ] {
        sqlx::query("INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES(?1,?2,?1,?3,'hash')").bind(id).bind(tenant).bind(format!("{id}@example.test")).execute(&pool).await.unwrap();
    }
    let resources = SqliteProductResourceRepo::new(pool.clone());
    resources.create(&CreateResourceParams{id:"policy".into(),tenant_id:"tenant".into(),kind:"alert_policy".into(),actor_id:"actor".into(),fields:ResourceWrite{code:"policy".into(),name:"失败告警".into(),description:None,parent_id:None,status:"draft".into(),properties:json!({"event_type":"task_failure","enabled":true,"window_seconds":3600,"threshold":1,"recipient_ids":["recipient"],"cooldown_seconds":600}),references:vec![ResourceReference{field:"recipient_ids".into(),target_kind:"user".into(),target_id:"recipient".into()}]}}).await.unwrap();
    for id in ["source", "target"] {
        sqlx::query("INSERT INTO datasource(id,tenant_id,name,db_type,host,port,created_by,health_status,last_test_at) VALUES(?1,'tenant',?1,'sqlite','local',0,'actor','healthy',?2)").bind(id).bind(Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO sync_task(id,tenant_id,name,source_id,target_id,created_by) VALUES('task','tenant','Task','source','target','actor')").execute(&pool).await.unwrap();
    (pool.clone(), SqliteAlertPolicyRepo::new(pool))
}

fn params(key: &str, threshold: i64, test: bool) -> EvaluateAlertPolicyParams {
    EvaluateAlertPolicyParams {
        tenant_id: "tenant".into(),
        actor_id: "actor".into(),
        policy_id: "policy".into(),
        policy_version: 1,
        policy_name: "失败告警".into(),
        event_type: "task_failure".into(),
        window_seconds: 3600,
        threshold,
        cooldown_seconds: 600,
        recipient_ids: vec!["recipient".into()],
        subject: "告警".into(),
        body: "任务失败".into(),
        channel: "in_app".into(),
        idempotency_key: key.into(),
        test_delivery: test,
    }
}

#[tokio::test]
async fn real_failures_trigger_once_then_cooldown_suppresses_without_fake_delivery() {
    let (pool, repo) = setup().await;
    sqlx::query("INSERT INTO task_run(id,task_id,status,start_time,end_time,created_at) VALUES('run','task','failed',?1,?1,?1)").bind(Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
    let first = repo.evaluate(&params("first", 1, false)).await.unwrap();
    assert_eq!(first.state, "triggered");
    assert_eq!(first.observed_count, 1);
    assert_eq!(first.deliveries.len(), 1);
    assert_eq!(first.deliveries[0].status, "pending");
    let replay = repo.evaluate(&params("first", 1, false)).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.id, first.id);
    let suppressed = repo.evaluate(&params("second", 1, false)).await.unwrap();
    assert_eq!(suppressed.state, "suppressed");
    assert!(suppressed.deliveries.is_empty());
    let page = repo
        .list(&AlertPolicyEvaluationFilter {
            tenant_id: "tenant".into(),
            policy_id: "policy".into(),
            limit: 20,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert!(
        sqlx::query("UPDATE alert_policy_evaluation SET state='test'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn within_threshold_has_no_outbox_but_test_delivery_is_persisted() {
    let (_pool, repo) = setup().await;
    let normal = repo.evaluate(&params("normal", 1, false)).await.unwrap();
    assert_eq!(normal.state, "within_threshold");
    assert!(normal.deliveries.is_empty());
    let test = repo.evaluate(&params("test", 1, true)).await.unwrap();
    assert_eq!(test.state, "test");
    assert_eq!(test.observed_count, 0);
    assert_eq!(test.deliveries.len(), 1);
}

#[tokio::test]
async fn keys_versions_and_tenants_are_enforced() {
    let (_pool, repo) = setup().await;
    repo.evaluate(&params("same", 1, true)).await.unwrap();
    assert!(matches!(
        repo.evaluate(&params("same", 2, true)).await,
        Err(BackendError::Conflict(_))
    ));
    let mut stale = params("stale", 1, true);
    stale.policy_version = 2;
    assert!(matches!(
        repo.evaluate(&stale).await,
        Err(BackendError::Conflict(_))
    ));
    let mut foreign = params("foreign", 1, true);
    foreign.recipient_ids = vec!["foreign".into()];
    assert!(matches!(
        repo.evaluate(&foreign).await,
        Err(BackendError::Forbidden(_))
    ));
}

#[tokio::test]
async fn audit_failure_rolls_back_evaluation_and_outbox() {
    let (pool, repo) = setup().await;
    sqlx::raw_sql("CREATE TRIGGER reject_alert_policy_audit BEFORE INSERT ON product_resource_audit WHEN NEW.resource_kind='alert_policy' AND NEW.operation='update' BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;").execute(&pool).await.unwrap();
    assert!(repo.evaluate(&params("rollback", 1, true)).await.is_err());
    let evaluations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_policy_evaluation")
        .fetch_one(&pool)
        .await
        .unwrap();
    let deliveries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_policy_delivery")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((evaluations, deliveries), (0, 0));
}

#[tokio::test]
async fn delivery_claim_is_fenced_and_persists_terminal_result() {
    let (pool, repo) = setup().await;
    let evaluation = repo.evaluate(&params("delivery", 1, true)).await.unwrap();
    let claim = repo
        .claim_delivery(&ClaimAlertDeliveryParams {
            worker_id: "worker-one".into(),
            lease_seconds: 30,
            max_attempts: 3,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.channel, "in_app");
    assert_eq!(claim.attempt_count, 1);
    assert!(
        repo.claim_delivery(&ClaimAlertDeliveryParams {
            worker_id: "worker-two".into(),
            lease_seconds: 30,
            max_attempts: 3,
        })
        .await
        .unwrap()
        .is_none()
    );
    let delivered = repo
        .finish_delivery(&FinishAlertDeliveryParams {
            tenant_id: "tenant".into(),
            delivery_id: claim.id,
            lease: AlertDeliveryLease {
                worker_id: claim.lease_owner,
                fence: claim.lease_fence,
            },
            outcome: AlertDeliveryOutcome::Delivered,
            error: None,
            retry_at: None,
        })
        .await
        .unwrap();
    assert_eq!(delivered.status, "delivered");
    assert!(delivered.delivered_at.is_some());
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_policy_delivery WHERE tenant_id='tenant' AND status='pending'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, 0);
    assert_eq!(evaluation.deliveries.len(), 1);
    let inbox_repo = SqliteMessageDeliveryRepo::new(pool);
    let inbox = inbox_repo
        .list_inbox(&dts_backend::repo::message_delivery::MessageInboxFilter {
            tenant_id: "tenant".into(),
            recipient_id: "recipient".into(),
            unread_only: true,
            limit: 20,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(inbox.total, 1);
    assert_eq!(inbox.data[0].source_kind, "alert_policy");
    let read = inbox_repo
        .mark_read("tenant", "recipient", &inbox.data[0].id)
        .await
        .unwrap();
    assert!(read.read_at.is_some());
}

#[tokio::test]
async fn durable_schedule_claim_advances_with_fence() {
    let (_pool, repo) = setup().await;
    assert_eq!(repo.sync_schedules().await.unwrap(), 1);
    let claim = repo
        .claim_scheduled(&ClaimScheduledAlertPolicyParams {
            worker_id: "schedule-one".into(),
            lease_seconds: 30,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.policy_id, "policy");
    assert!(
        repo.claim_scheduled(&ClaimScheduledAlertPolicyParams {
            worker_id: "schedule-two".into(),
            lease_seconds: 30,
        })
        .await
        .unwrap()
        .is_none()
    );
    repo.finish_scheduled(&FinishScheduledAlertPolicyParams {
        tenant_id: claim.tenant_id,
        policy_id: claim.policy_id,
        lease: AlertDeliveryLease {
            worker_id: claim.lease_owner,
            fence: claim.lease_fence,
        },
        next_evaluation_at: Utc::now() + chrono::Duration::minutes(5),
        error: None,
    })
    .await
    .unwrap();
    assert!(
        repo.claim_scheduled(&ClaimScheduledAlertPolicyParams {
            worker_id: "schedule-three".into(),
            lease_seconds: 30,
        })
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn retry_backoff_reaches_explicit_max_attempt_terminal_state() {
    let (pool, repo) = setup().await;
    repo.evaluate(&params("retry", 1, true)).await.unwrap();
    let claim = repo
        .claim_delivery(&ClaimAlertDeliveryParams {
            worker_id: "retry-worker".into(),
            lease_seconds: 30,
            max_attempts: 1,
        })
        .await
        .unwrap()
        .unwrap();
    repo.finish_delivery(&FinishAlertDeliveryParams {
        tenant_id: claim.tenant_id,
        delivery_id: claim.id.clone(),
        lease: AlertDeliveryLease {
            worker_id: claim.lease_owner,
            fence: claim.lease_fence,
        },
        outcome: AlertDeliveryOutcome::Retry,
        error: Some("temporary".into()),
        retry_at: Some(Utc::now() - chrono::Duration::seconds(1)),
    })
    .await
    .unwrap();
    assert!(
        repo.claim_delivery(&ClaimAlertDeliveryParams {
            worker_id: "retry-worker-two".into(),
            lease_seconds: 30,
            max_attempts: 1,
        })
        .await
        .unwrap()
        .is_none()
    );
    let row: (String, String) = sqlx::query_as(
        "SELECT status,last_error FROM alert_policy_delivery WHERE tenant_id='tenant' AND id=?1",
    )
    .bind(claim.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "failed");
    assert_eq!(row.1, "已达到最大重试次数");
}
