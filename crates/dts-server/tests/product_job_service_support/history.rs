// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[actix_web::test]
async fn historic_detail_and_idempotent_submission_recheck_reader_access_without_erasing_results() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = service(&ctx, &actor, query.clone());
    let source = source(
        &ctx,
        &actor,
        "sql_document",
        json!({"sql":"SELECT $name AS value",
        "parameters":[{"name":"name","data_type":"VARCHAR","required":true}]}),
    )
    .await;
    let version = publish(&service, &actor, &source).await;
    let request = || RunRequest {
        expected_version: version,
        idempotency_key: "historical-sensitive-parameter".into(),
        parameters: json!({"name":"sensitive-business-value"}),
    };
    let submitted = service
        .enqueue(&actor, "sql_document", &source.id, request())
        .await
        .unwrap();
    let job_id = submitted.job.id;
    service.run_once().await.unwrap();
    let allowed = service.get(&actor, &job_id).await.unwrap();
    assert_eq!(allowed.result_access, ResultAccess::Authorized);
    assert_eq!(allowed.nodes[0].result.as_ref().unwrap()["row_count"], 1);
    let summary = service
        .list(
            &actor,
            JobListQuery {
                source_kind: Some("sql_document".into()),
                source_id: Some(source.id.clone()),
                status: None,
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(summary.data[0].status, ProductJobStatus::Succeeded);
    assert_eq!(summary.data[0].result_access, ResultAccess::Summary);
    assert!(summary.data[0].nodes[0].result.is_none());
    let summary_json = serde_json::to_string(&summary).unwrap();
    assert!(!summary_json.contains("sensitive-business-value"));
    let role = ctx
        .role_repo
        .list_all(Some(&actor.tenant_id))
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.name == "tenant_admin")
        .unwrap();
    ctx.role_repo
        .update(
            &role.id,
            &dts_backend::types::UpdateRoleParams {
                name: None,
                permissions: Some(
                    role.permissions
                        .into_iter()
                        .filter(|p| p != "data_security:all_data")
                        .collect(),
                ),
            },
        )
        .await
        .unwrap();
    let restricted = service.get(&actor, &job_id).await.unwrap();
    assert_eq!(restricted.status, ProductJobStatus::Succeeded);
    assert_eq!(restricted.result_access, ResultAccess::Restricted);
    assert!(restricted.result.is_none());
    assert!(restricted.nodes[0].result.is_none());
    let repeated = service
        .enqueue(&actor, "sql_document", &source.id, request())
        .await
        .unwrap();
    assert!(!repeated.created);
    let response = service.submission_view(&actor, repeated).await.unwrap();
    assert_eq!(response.job.result_access, ResultAccess::Restricted);
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains("sensitive-business-value")
    );
    let no_guard = ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query,
        QueryAccess {
            tenant_id: Some(actor.tenant_id.clone()),
        },
    );
    let unavailable = no_guard.get(&actor, &job_id).await.unwrap();
    assert_eq!(
        unavailable.result_access_reason,
        Some("authorization_unconfigured")
    );
    let raw = ctx
        .product_job_repo
        .get(&dts_backend::repo::product_job::ProductJobLookup {
            tenant_id: actor.tenant_id.clone(),
            job_id,
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw.status, ProductJobStatus::Succeeded);
    assert!(raw.result.is_some());
    assert!(raw.nodes[0].result.is_some());
    assert_eq!(raw.parameters["name"], "sensitive-business-value");
    let audit = ctx
        .data_access_repo
        .list_audit(&dts_backend::repo::data_access::DataAccessAuditFilter {
            tenant_id: actor.tenant_id,
            actor_id: actor.sub,
            limit: 50,
            offset: 0,
        })
        .await
        .unwrap();
    assert!(audit.data.iter().any(|a| a.outcome == "denied"));
}
