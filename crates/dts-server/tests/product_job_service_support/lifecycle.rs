use super::*;

#[actix_web::test]
async fn invalid_periodic_policy_never_publishes_or_enqueues_a_real_dag() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let jobs = service(&ctx, &actor, query.clone());
    for schedule in [
        json!({"cron_expression":"0 25 * * *","timezone":"UTC"}),
        json!({"cron_expression":"*/0 * * * *","timezone":"UTC"}),
        json!({"cron_expression":"0 8 * * *","timezone":"Asia/DoesNotExist"}),
        json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":1.5}),
    ] {
        let mut properties = schedule;
        properties["definition"] =
            json!({"nodes":[{"id":"query","name":"Query","sql":"SELECT 1","depends_on":[]}]});
        let source = source(&ctx, &actor, "development_workflow", properties).await;
        assert!(
            jobs.validate(&actor, &source.kind, &source.id, source.version)
                .await
                .is_err()
        );
        assert!(
            jobs.publish(&actor, &source.kind, &source.id, source.version)
                .await
                .is_err()
        );
        let unchanged = ctx
            .product_resource_repo
            .get(&actor.tenant_id, &source.kind, &source.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.status, "draft");
        assert_eq!(unchanged.version, source.version);
    }
    assert!(!jobs.run_once().await.unwrap());
    assert!(query.calls.lock().unwrap().is_empty());
}

#[actix_web::test]
async fn withdrawing_definition_blocks_new_execution_and_preserves_observed_history() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = service(&ctx, &actor, query.clone());
    for (kind, properties) in [
        ("sql_document", json!({"sql":"SELECT 1"})),
        (
            "development_workflow",
            json!({"definition":{"nodes":[{"id":"first","name":"First","sql":"SELECT 1","depends_on":[]}]},"cron_expression":"0 0 * * * *","timezone":"UTC","retry_limit":0}),
        ),
    ] {
        let source = source(&ctx, &actor, kind, properties).await;
        let version = publish(&service, &actor, &source).await;
        let job = service
            .enqueue(
                &actor,
                kind,
                &source.id,
                RunRequest {
                    expected_version: version,
                    idempotency_key: format!("withdraw-{kind}"),
                    parameters: json!({}),
                },
            )
            .await
            .unwrap()
            .job;
        let result = service
            .withdraw(&actor, kind, &source.id, version)
            .await
            .unwrap();
        assert_eq!(result["resource"]["status"], "inactive");
        assert_eq!(result["resource"]["version"], version + 1);
        assert_eq!(result["new_execution_enabled"], false);
        assert_eq!(
            service.get(&actor, &job.id).await.unwrap().status,
            ProductJobStatus::Queued
        );
        assert!(
            service
                .withdraw(&actor, kind, &source.id, version)
                .await
                .is_err()
        );
        assert!(
            service
                .enqueue(
                    &actor,
                    kind,
                    &source.id,
                    RunRequest {
                        expected_version: version + 1,
                        idempotency_key: format!("after-withdraw-{kind}"),
                        parameters: json!({}),
                    }
                )
                .await
                .is_err()
        );
        assert!(service.run_once().await.unwrap());
        let actual = service.get(&actor, &job.id).await.unwrap();
        assert_eq!(actual.status, ProductJobStatus::Failed);
        assert!(actual.result.is_none());
        assert!(
            actual
                .nodes
                .iter()
                .all(|node| node.status == ProductJobNodeStatus::Skipped)
        );
    }
    assert!(query.calls.lock().unwrap().is_empty());
}
