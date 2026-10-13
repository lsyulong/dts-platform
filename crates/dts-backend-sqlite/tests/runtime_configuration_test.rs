mod runtime_configuration_support;
use dts_backend::{
    BackendError,
    repo::{product_job::*, runtime_configuration::*},
};
use runtime_configuration_support::*;
use serde_json::json;

#[tokio::test]
async fn audit_failure_rolls_back_publication_and_receipt_and_old_witness_is_rejected() {
    let (pool, repo, _) = setup().await;
    let cfg = resource(&pool, "development_config", "cfg").await;
    let request = PublishRuntimeConfigurationParams {
        lookup: lookup(),
        expected_version: 1,
        reason: "Release under audit".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        validation: Some(ValidatedRuntimePolicy::from_validated(&cfg, None).unwrap()),
    };
    sqlx::raw_sql("CREATE TRIGGER reject_config_audit BEFORE INSERT ON product_resource_audit BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;").execute(&pool).await.unwrap();
    assert!(repo.publish(&request).await.is_err());
    assert_eq!(
        resource(&pool, "development_config", "cfg").await.version,
        1
    );
    assert_eq!(count(&pool, "runtime_configuration_publication").await, 0);
    assert_eq!(count(&pool, "runtime_configuration_receipt").await, 0);
    sqlx::query("DROP TRIGGER reject_config_audit")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.concurrency',2) WHERE id='cfg'").execute(&pool).await.unwrap();
    assert!(repo.publish(&request).await.is_err());
    assert_eq!(count(&pool, "product_resource_audit").await, 0);
}

#[tokio::test]
async fn publication_is_atomic_immutable_and_idempotent_without_restoring_withdrawn_state() {
    let (pool, repo, _) = setup().await;
    let cfg = resource(&pool, "development_config", "cfg").await;
    let mut p = PublishRuntimeConfigurationParams {
        lookup: lookup(),
        expected_version: 1,
        reason: "Release reviewed limits".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        validation: None,
    };
    assert!(matches!(
        repo.publish(&p).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(count(&pool, "product_resource_audit").await, 0);
    p.validation = Some(ValidatedRuntimePolicy::from_validated(&cfg, None).unwrap());
    let published = repo.publish(&p).await.unwrap();
    assert_eq!(published.resource.version, 2);
    let replay = repo.publish(&p).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.receipt_id, published.receipt_id);
    assert_eq!(count(&pool, "product_resource_audit").await, 1);
    repo.withdraw(&WithdrawRuntimeConfigurationParams {
        lookup: lookup(),
        expected_version: 2,
        reason: "Retire limits".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    })
    .await
    .unwrap();
    assert!(repo.publish(&p).await.unwrap().replayed);
    assert_eq!(
        resource(&pool, "development_config", "cfg").await.status,
        "inactive"
    );
    assert_eq!(count(&pool, "runtime_configuration_publication").await, 1);
    assert!(
        sqlx::query("UPDATE runtime_configuration_publication SET snapshot='{}'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM runtime_configuration_receipt")
            .execute(&pool)
            .await
            .is_err()
    );
    p.reason = "Changed semantics".into();
    assert!(matches!(
        repo.publish(&p).await,
        Err(BackendError::Conflict(_))
    ));
}

#[tokio::test]
async fn real_source_binding_rejects_forged_evidence_and_withdrawal_keeps_history() {
    let (pool, repo, jobs) = setup().await;
    let publication = publish_config(&pool, &repo).await;
    let src = publish_source(&pool, &repo, "one").await;
    let input = repo
        .current_source(&source_lookup(
            "one",
            src.version,
            RuntimeSourceAccess::Execute,
        ))
        .await
        .unwrap();
    assert_eq!(input.evidence, publication.evidence);
    let evidence = input.evidence.unwrap();
    let mut bad = enqueue(&src, evidence.clone());
    bad.definition_snapshot["runtime_configuration"]["concurrency"] = json!(16);
    assert!(jobs.enqueue(&bad).await.is_err());
    assert_eq!(count(&pool, "product_job").await, 0);
    let request = enqueue(&src, evidence.clone());
    jobs.enqueue(&request).await.unwrap();
    assert!(jobs.enqueue(&request).await.unwrap().created == false);
    repo.withdraw(&WithdrawRuntimeConfigurationParams {
        lookup: lookup(),
        expected_version: 2,
        reason: "Disable configuration".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    })
    .await
    .unwrap();
    assert!(
        repo.verify(&VerifyRuntimeConfigurationParams {
            source: source_lookup("one", 2, RuntimeSourceAccess::Read),
            evidence: Some(evidence)
        })
        .await
        .is_err()
    );
    assert!(jobs.enqueue(&request).await.is_err());
    assert_eq!(count(&pool, "product_job").await, 1);
    assert_eq!(count(&pool, "runtime_job_configuration_binding").await, 1);
    assert!(
        sqlx::query("DELETE FROM runtime_source_configuration_binding")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_workers_share_one_actual_slot_and_expiry_fences_the_old_worker() {
    let (pool, repo, jobs) = setup().await;
    let evidence = publish_config(&pool, &repo).await.evidence.unwrap();
    for id in ["one", "two", "three"] {
        let src = publish_source(&pool, &repo, id).await;
        jobs.enqueue(&enqueue(&src, evidence.clone()))
            .await
            .unwrap();
    }
    let a = claim("worker-a");
    let b = claim("worker-b");
    let (one, two) = tokio::join!(jobs.claim(&a), jobs.claim(&b));
    let one = one.unwrap();
    let two = two.unwrap();
    assert_ne!(one.is_some(), two.is_some());
    let active = one.or(two).unwrap();
    assert!(jobs.claim(&claim("worker-c")).await.unwrap().is_none());
    sqlx::query("UPDATE product_job SET lease_expires_at='2000-01-01T00:00:00Z' WHERE id=?1")
        .bind(&active.id)
        .execute(&pool)
        .await
        .unwrap();
    let recovered = jobs.claim(&claim("recovery")).await.unwrap().unwrap();
    assert!(
        jobs.heartbeat(&HeartbeatProductJobParams {
            tenant_id: "a".into(),
            job_id: active.id.clone(),
            lease: lease(&active),
            lease_seconds: 30
        })
        .await
        .is_err()
    );
    let running:i64=sqlx::query_scalar("SELECT COUNT(*) FROM product_job WHERE status='running' AND julianday(lease_expires_at)>julianday('now')").fetch_one(&pool).await.unwrap();
    assert_eq!(running, 1);
    jobs.finish(&FinishProductJobParams {
        tenant_id: "a".into(),
        job_id: recovered.id.clone(),
        lease: lease(&recovered),
        outcome: ProductJobOutcome::Failed,
        result: None,
        error: Some("Actual worker failure".into()),
    })
    .await
    .unwrap();
    assert!(jobs.claim(&claim("next")).await.unwrap().is_some());
}

#[tokio::test]
async fn actual_permissions_tenant_and_source_scope_are_checked_before_effects() {
    let (pool, repo, _) = setup().await;
    publish_config(&pool, &repo).await;
    let mut wrong = lookup();
    wrong.scope.tenant_id = "b".into();
    assert!(matches!(
        repo.get(&wrong).await,
        Err(BackendError::Forbidden(_))
    ));
    let mut read = lookup();
    read.scope.actor_id = "reader".into();
    assert!(repo.get(&read).await.is_ok());
    assert!(matches!(
        repo.withdraw(&WithdrawRuntimeConfigurationParams {
            lookup: read,
            expected_version: 2,
            reason: "Unauthorized withdrawal".into(),
            idempotency_key: uuid::Uuid::new_v4().to_string()
        })
        .await,
        Err(BackendError::Forbidden(_))
    ));
    insert(
        &pool,
        "development_config",
        "scoped",
        json!({"scope":"source","source_kind":"sql_document","source_id":"one","variables":{}}),
    )
    .await;
    let config = resource(&pool, "development_config", "scoped").await;
    let src = resource(&pool, "sql_document", "one").await;
    repo.publish(&PublishRuntimeConfigurationParams {
        lookup: RuntimeConfigurationLookup {
            scope: scope(),
            configuration_id: "scoped".into(),
        },
        expected_version: 1,
        reason: "Source-specific configuration".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        validation: Some(ValidatedRuntimePolicy::from_validated(&config, Some(&src)).unwrap()),
    })
    .await
    .unwrap();
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.configuration_id','scoped') WHERE id='two'").execute(&pool).await.unwrap();
    assert!(
        repo.current_source(&source_lookup("two", 1, RuntimeSourceAccess::Publish))
            .await
            .is_err()
    );
    assert_eq!(resource(&pool, "sql_document", "two").await.status, "draft");
}
