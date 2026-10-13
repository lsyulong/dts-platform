// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_job::*};
use serde_json::json;

mod product_job_support;
use product_job_support::*;

#[tokio::test]
async fn enqueue_is_queued_snapshot_immutable_idempotent_and_source_protected() {
    let (pool, repo) = setup().await;
    let params = request("job");
    let first = repo.enqueue(&params).await.expect("enqueue");
    assert!(first.created);
    assert_eq!(first.job.status, ProductJobStatus::Queued);
    assert_eq!(first.job.definition_snapshot, params.definition_snapshot);
    let second = repo.enqueue(&params).await.expect("idempotent");
    assert!(!second.created);
    assert_eq!(second.job.id, first.job.id);
    assert!(
        sqlx::query("DELETE FROM product_resource WHERE id = 'a-sql'")
            .execute(&pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE product_resource SET version = 2 WHERE id = 'a-sql'")
        .execute(&pool)
        .await
        .expect("source advanced");
    assert!(
        !repo
            .enqueue(&params)
            .await
            .expect("immutable historical request")
            .created
    );
    assert!(matches!(
        repo.enqueue(&request("different")).await,
        Err(BackendError::Conflict(_))
    ));
    let mut conflicting = params;
    conflicting.actor_id = "a-other".into();
    assert!(matches!(
        repo.enqueue(&conflicting).await,
        Err(BackendError::Conflict(_))
    ));
    conflicting.actor_id = "a-owner".into();
    conflicting.parameters = json!({"limit": 2});
    assert!(matches!(
        repo.enqueue(&conflicting).await,
        Err(BackendError::Conflict(_))
    ));
}

#[tokio::test]
async fn invalid_source_actor_and_dependency_graph_do_not_create_runs() {
    let (_pool, repo) = setup().await;
    let mut params = request("bad");
    params.source_id = "b-sql".into();
    assert!(repo.enqueue(&params).await.is_err());
    params.source_id = "a-sql".into();
    params.actor_id = "b-owner".into();
    assert!(repo.enqueue(&params).await.is_err());
    params.actor_id = "a-owner".into();
    params.nodes = vec![
        definition("one", vec!["two"]),
        definition("two", vec!["one"]),
    ];
    assert!(matches!(
        repo.enqueue(&params).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    params.nodes = vec![definition("one", vec!["missing"])];
    assert!(repo.enqueue(&params).await.is_err());
    params.nodes.clear();
    assert!(repo.enqueue(&params).await.is_err());
    assert!(repo.get(&lookup("bad")).await.expect("get").is_none());
}

#[tokio::test]
async fn concurrent_enqueues_and_claims_enforce_one_active_source_and_one_owner() {
    let (_pool, repo) = setup().await;
    let one = request("one");
    let two = request("two");
    let (one, two) = tokio::join!(repo.enqueue(&one), repo.enqueue(&two));
    assert_ne!(one.is_ok(), two.is_ok());
    let a = claim_params("worker-a");
    let b = claim_params("worker-b");
    let (a, b) = tokio::join!(repo.claim(&a), repo.claim(&b));
    assert_ne!(a.expect("claim a").is_some(), b.expect("claim b").is_some());
}

#[tokio::test]
async fn claim_respects_executor_kind_whitelist_and_success_requires_real_node_transitions() {
    let (_pool, repo) = setup().await;
    let mut params = request("workflow");
    params.source_kind = "development_workflow".into();
    params.source_id = "a-workflow".into();
    repo.enqueue(&params).await.expect("workflow");
    assert!(
        repo.claim(&claim_params("sql-worker"))
            .await
            .expect("kind filter")
            .is_none()
    );
    let mut claim = claim_params("workflow-worker");
    claim.source_kinds = vec!["development_workflow".into()];
    let job = repo.claim(&claim).await.expect("claim").expect("job");
    assert!(
        repo.finish(&finish(&job, ProductJobOutcome::Succeeded))
            .await
            .is_err()
    );
    repo.start_node(&start(&job, "one")).await.expect("start");
    repo.finish_node(&node_finish(&job, "one", ProductJobOutcome::Succeeded))
        .await
        .expect("node result");
    let completed = repo
        .finish(&finish(&job, ProductJobOutcome::Succeeded))
        .await
        .expect("finish");
    assert_eq!(completed.status, ProductJobStatus::Succeeded);
    assert_eq!(completed.result, Some(json!({"rows": 1})));
    assert!(completed.lease_owner.is_none());
    assert!(completed.finished_at.is_some());
}

#[tokio::test]
async fn failed_dependency_skips_descendants_and_no_node_can_start_before_dependencies() {
    let (_pool, repo) = setup().await;
    let mut params = request("dag");
    params.nodes = vec![
        definition("one", vec![]),
        definition("two", vec!["one"]),
        definition("three", vec!["two"]),
        definition("independent", vec![]),
    ];
    repo.enqueue(&params).await.expect("dag");
    let job = repo
        .claim(&claim_params("worker"))
        .await
        .expect("claim")
        .expect("job");
    assert!(repo.start_node(&start(&job, "two")).await.is_err());
    repo.start_node(&start(&job, "one")).await.expect("start");
    repo.finish_node(&node_finish(&job, "one", ProductJobOutcome::Failed))
        .await
        .expect("failed node");
    let observed = repo.get(&lookup("dag")).await.expect("get").expect("job");
    assert!(
        observed
            .nodes
            .iter()
            .filter(|node| node.id == "two" || node.id == "three")
            .all(|node| node.status == ProductJobNodeStatus::Skipped)
    );
    let finished = repo
        .finish(&finish(&job, ProductJobOutcome::Failed))
        .await
        .expect("failed job");
    assert!(
        finished
            .nodes
            .iter()
            .any(|node| node.id == "independent" && node.status == ProductJobNodeStatus::Skipped)
    );
}

#[tokio::test]
async fn queued_cancel_is_terminal_running_cancel_blocks_new_nodes_and_success() {
    let (_pool, repo) = setup().await;
    repo.enqueue(&request("queued")).await.expect("enqueue");
    let queued = repo
        .request_cancel(&CancelProductJobParams {
            tenant_id: "a".into(),
            job_id: "queued".into(),
            actor_id: "a-owner".into(),
        })
        .await
        .expect("cancel");
    assert_eq!(queued.status, ProductJobStatus::Cancelled);
    assert_eq!(queued.nodes[0].status, ProductJobNodeStatus::Cancelled);
    assert!(
        repo.claim(&claim_params("worker"))
            .await
            .expect("claim")
            .is_none()
    );
    repo.enqueue(&request("running"))
        .await
        .expect("enqueue again");
    let running = repo
        .claim(&claim_params("worker"))
        .await
        .expect("claim")
        .expect("job");
    repo.start_node(&start(&running, "one"))
        .await
        .expect("start");
    repo.request_cancel(&CancelProductJobParams {
        tenant_id: "a".into(),
        job_id: "running".into(),
        actor_id: "a-owner".into(),
    })
    .await
    .expect("cancel running");
    assert!(
        repo.finish(&finish(&running, ProductJobOutcome::Succeeded))
            .await
            .is_err()
    );
    repo.finish_node(&node_finish(&running, "one", ProductJobOutcome::Cancelled))
        .await
        .expect("cancel node");
    assert_eq!(
        repo.finish(&finish(&running, ProductJobOutcome::Cancelled))
            .await
            .expect("cancel done")
            .status,
        ProductJobStatus::Cancelled
    );
}

#[tokio::test]
async fn readonly_reclaim_increments_fence_preserves_success_and_rejects_every_old_worker_write() {
    let (pool, repo) = setup().await;
    let mut params = request("retry");
    params.nodes = vec![definition("one", vec![]), definition("two", vec!["one"])];
    repo.enqueue(&params).await.expect("enqueue");
    let old = repo
        .claim(&claim_params("worker-a"))
        .await
        .expect("claim")
        .expect("job");
    repo.start_node(&start(&old, "one"))
        .await
        .expect("one start");
    repo.finish_node(&node_finish(&old, "one", ProductJobOutcome::Succeeded))
        .await
        .expect("one finish");
    repo.start_node(&start(&old, "two"))
        .await
        .expect("two start");
    expire(&pool, "retry").await;
    let new = repo
        .claim(&claim_params("worker-b"))
        .await
        .expect("reclaim")
        .expect("job");
    assert_eq!(new.lease_fence, old.lease_fence + 1);
    assert_eq!(new.attempt, 2);
    assert!(
        new.nodes
            .iter()
            .any(|node| node.id == "one" && node.status == ProductJobNodeStatus::Succeeded)
    );
    assert!(
        new.nodes
            .iter()
            .any(|node| node.id == "two" && node.status == ProductJobNodeStatus::Pending)
    );
    assert!(
        repo.heartbeat(&HeartbeatProductJobParams {
            tenant_id: "a".into(),
            job_id: old.id.clone(),
            lease: lease(&old),
            lease_seconds: 30
        })
        .await
        .is_err()
    );
    assert!(repo.start_node(&start(&old, "two")).await.is_err());
    assert!(
        repo.finish_node(&node_finish(&old, "two", ProductJobOutcome::Succeeded))
            .await
            .is_err()
    );
    assert!(
        repo.finish(&finish(&old, ProductJobOutcome::Failed))
            .await
            .is_err()
    );
    repo.start_node(&start(&new, "two"))
        .await
        .expect("new start");
    repo.finish_node(&node_finish(&new, "two", ProductJobOutcome::Succeeded))
        .await
        .expect("new finish");
    assert_eq!(
        repo.finish(&finish(&new, ProductJobOutcome::Succeeded))
            .await
            .expect("new done")
            .status,
        ProductJobStatus::Succeeded
    );
}

#[tokio::test]
async fn expired_nonretryable_run_requires_explicit_reconciliation_before_new_run() {
    let (pool, repo) = setup().await;
    let mut params = request("uncertain");
    params.can_retry = false;
    repo.enqueue(&params).await.expect("enqueue");
    let job = repo
        .claim(&claim_params("worker"))
        .await
        .expect("claim")
        .expect("job");
    repo.start_node(&start(&job, "one")).await.expect("start");
    expire(&pool, &job.id).await;
    assert!(
        repo.claim(&claim_params("next"))
            .await
            .expect("reap")
            .is_none()
    );
    let observed = repo.get(&lookup(&job.id)).await.expect("get").expect("job");
    assert_eq!(observed.status, ProductJobStatus::ReconciliationRequired);
    assert_eq!(
        observed.nodes[0].status,
        ProductJobNodeStatus::ReconciliationRequired
    );
    assert!(repo.enqueue(&request("next-job")).await.is_err());
    assert!(
        repo.finish_node(&node_finish(&job, "one", ProductJobOutcome::Succeeded))
            .await
            .is_err()
    );
    repo.reconcile(&ReconcileProductJobParams {
        tenant_id: "a".into(),
        job_id: job.id,
        actor_id: "a-owner".into(),
        expected_fence: observed.lease_fence,
        outcome: ProductJobReconciliation::Failed,
        reason: "checked external state; no committed output".into(),
    })
    .await
    .expect("reconciliation");
    assert!(
        repo.enqueue(&request("next-job"))
            .await
            .expect("next job")
            .created
    );
}

#[tokio::test]
async fn reads_filter_actual_actor_and_tenant_before_pagination_and_results_are_bounded() {
    let (_pool, repo) = setup().await;
    repo.enqueue(&request("job")).await.expect("enqueue");
    let filter = ProductJobFilter {
        tenant_id: "a".into(),
        actor_id: Some("a-other".into()),
        source_kind: None,
        source_id: None,
        status: None,
        limit: 10,
        offset: 0,
    };
    assert_eq!(repo.list(&filter).await.expect("actor filter").total, 0);
    assert!(
        repo.get(&ProductJobLookup {
            tenant_id: "b".into(),
            job_id: "job".into(),
            actor_id: None
        })
        .await
        .expect("tenant filter")
        .is_none()
    );
    assert!(
        repo.get(&ProductJobLookup {
            tenant_id: "a".into(),
            job_id: "job".into(),
            actor_id: Some("a-other".into())
        })
        .await
        .expect("actor get")
        .is_none()
    );
    let job = repo
        .claim(&claim_params("worker"))
        .await
        .expect("claim")
        .expect("job");
    repo.start_node(&start(&job, "one")).await.expect("start");
    let mut completion = node_finish(&job, "one", ProductJobOutcome::Succeeded);
    completion.result = Some(json!({"payload": "x".repeat(256 * 1024)}));
    assert!(matches!(
        repo.finish_node(&completion).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    assert_eq!(
        repo.get(&lookup("job"))
            .await
            .expect("get")
            .expect("job")
            .nodes[0]
            .status,
        ProductJobNodeStatus::Running
    );
}
