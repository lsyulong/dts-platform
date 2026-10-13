// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;
#[path = "product_job_service_support/history.rs"]
mod history;
#[path = "product_job_service_support/lifecycle.rs"]
mod lifecycle;
#[path = "product_job_service_support/masking.rs"]
mod masking;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use actix_web::{App, test, web};
use dts_backend::types::{CreateRoleParams, CreateUserParams};
use dts_backend::{
    CreateResourceParams, ProductResource, ResourceWrite,
    repo::product_job::{ProductJobNodeStatus, ProductJobStatus},
};
use dts_server::{
    api::{product_jobs, query::QueryAccess},
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::QueryAuthorizationService,
        product_jobs::{
            JobListQuery, ProductJobService, ReadOnlyQueryExecutor, ResultAccess, RetryRequest,
            RunRequest,
        },
        query_svc::{ColumnMeta, QueryResult},
    },
};
use serde_json::{Value, json};

#[derive(Default)]
struct ObservedQuery {
    calls: Mutex<Vec<String>>,
    fail: AtomicBool,
    oversized: AtomicBool,
}

#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for ObservedQuery {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        self.calls.lock().unwrap().push(sql.into());
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::BadRequest("controlled execution failure".into()));
        }
        let row = if self.oversized.load(Ordering::SeqCst) {
            json!({"value":"x".repeat(300 * 1024)})
        } else {
            json!({"value":1})
        };
        Ok(QueryResult {
            columns: vec![ColumnMeta {
                name: "value".into(),
                data_type: "Int64".into(),
                is_numeric: true,
            }],
            rows: vec![row],
            row_count: 1,
            suggested_chart: None,
        })
    }
}

async fn identity(ctx: &common::TestBackend) -> (AuthIdentity, String) {
    let token = common::admin_token(ctx).await;
    (ctx.auth.authenticate(&token).await.unwrap(), token)
}

fn service(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    query: Arc<ObservedQuery>,
) -> ProductJobService {
    bound_service(ctx, actor, query)
}

fn bound_service(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    query: Arc<dyn ReadOnlyQueryExecutor>,
) -> ProductJobService {
    ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query,
        QueryAccess {
            tenant_id: Some(actor.tenant_id.clone()),
        },
    )
    .with_query_authorization(Arc::new(QueryAuthorizationService::new(
        ctx.data_access_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        QueryAccess {
            tenant_id: Some(actor.tenant_id.clone()),
        },
    )))
    .with_masking(masking::service(ctx))
}

async fn source(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    kind: &str,
    properties: Value,
) -> ProductResource {
    ctx.product_resource_repo
        .create(&CreateResourceParams {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: actor.tenant_id.clone(),
            kind: kind.into(),
            actor_id: actor.sub.clone(),
            fields: ResourceWrite {
                code: format!("definition_{}", uuid::Uuid::new_v4().simple()),
                name: "Execution fixture".into(),
                description: None,
                parent_id: None,
                status: "draft".into(),
                properties,
                references: vec![],
            },
        })
        .await
        .unwrap()
}

async fn publish(
    service: &ProductJobService,
    actor: &AuthIdentity,
    source: &ProductResource,
) -> i64 {
    let result = service
        .publish(actor, &source.kind, &source.id, source.version)
        .await
        .unwrap();
    assert_eq!(
        result["publication_scope"],
        "validated_read_only_definition"
    );
    assert_eq!(result["executed"], false);
    assert_eq!(result["schedule_registered"], false);
    result["resource"]["version"].as_i64().unwrap()
}

#[actix_web::test]
async fn http_submission_is_idempotent_and_only_worker_observation_can_finish_the_job() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, token) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = service(&ctx, &actor, query.clone());
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT $name AS value","parameters":[{"name":"name","data_type":"VARCHAR","required":true}]})).await;
    let version = publish(&service, &actor, &source).await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .app_data(web::Data::new(service.clone()))
            .route(
                "/product/{kind}/{id}/jobs",
                web::post().to(product_jobs::enqueue),
            ),
    )
    .await;
    let uri = format!("/product/sql_document/{}/jobs", source.id);
    let mut id = None;
    for created in [true, false] {
        let response = test::call_service(&app, test::TestRequest::post().uri(&uri).insert_header(("Authorization", format!("Bearer {token}"))).set_json(json!({"expected_version":version,"idempotency_key":"submit-idempotent","parameters":{"name":"Alice's"}})).to_request()).await;
        assert_eq!(response.status(), 202);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["created"], created);
        assert_eq!(body["job"]["status"], "queued");
        assert!(body["job"]["result"].is_null());
        assert_eq!(body["job"]["nodes"][0]["status"], "pending");
        let current = body["job"]["id"].as_str().unwrap().to_owned();
        if let Some(previous) = &id {
            assert_eq!(previous, &current);
        }
        id = Some(current);
    }
    assert!(query.calls.lock().unwrap().is_empty());
    assert!(service.run_once().await.unwrap());
    let job = service.get(&actor, &id.unwrap()).await.unwrap();
    assert_eq!(job.status, ProductJobStatus::Succeeded);
    assert_eq!(job.nodes[0].status, ProductJobNodeStatus::Succeeded);
    assert_eq!(
        job.nodes[0].result.as_ref().unwrap()["rows"],
        json!([{"value":1}])
    );
    assert_eq!(
        query.calls.lock().unwrap().as_slice(),
        ["SELECT 'Alice''s' AS value"]
    );
    let audit = ctx
        .data_access_repo
        .list_audit(&dts_backend::repo::data_access::DataAccessAuditFilter {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            limit: 50,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(audit.total, 2);
    assert_eq!(audit.data[0].outcome, "succeeded");
}

#[actix_web::test]
async fn failed_node_skips_downstream_and_retry_creates_a_new_authorized_run() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    query.fail.store(true, Ordering::SeqCst);
    let service = service(&ctx, &actor, query.clone());
    let source = source(
        &ctx,
        &actor,
        "development_workflow",
        json!({"definition":{"nodes":[
        {"id":"upstream","name":"Upstream","sql":"SELECT 1","depends_on":[]},
        {"id":"downstream","name":"Downstream","sql":"SELECT 2","depends_on":["upstream"]}
    ]},"cron_expression":"0 0 * * * *","timezone":"Asia/Shanghai"}),
    )
    .await;
    let version = publish(&service, &actor, &source).await;
    let first = service
        .enqueue(
            &actor,
            "development_workflow",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "first-run".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    service.run_once().await.unwrap();
    let failed = service.get(&actor, &first.id).await.unwrap();
    assert_eq!(failed.status, ProductJobStatus::Failed);
    assert_eq!(
        failed
            .nodes
            .iter()
            .find(|node| node.id == "upstream")
            .unwrap()
            .status,
        ProductJobNodeStatus::Failed
    );
    assert_eq!(
        failed
            .nodes
            .iter()
            .find(|node| node.id == "downstream")
            .unwrap()
            .status,
        ProductJobNodeStatus::Skipped
    );
    assert_eq!(query.calls.lock().unwrap().len(), 1);
    query.fail.store(false, Ordering::SeqCst);
    let next = service
        .retry(
            &actor,
            &first.id,
            RetryRequest {
                idempotency_key: "explicit-retry".into(),
            },
        )
        .await
        .unwrap()
        .job;
    assert_ne!(next.id, first.id);
    assert_eq!(next.status, ProductJobStatus::Queued);
    assert_eq!(next.definition_snapshot, first.definition_snapshot);
    service.run_once().await.unwrap();
    assert_eq!(
        service.get(&actor, &next.id).await.unwrap().status,
        ProductJobStatus::Succeeded
    );
    assert_eq!(
        query.calls.lock().unwrap().as_slice(),
        ["SELECT 1", "SELECT 1", "SELECT 2"]
    );
}

#[actix_web::test]
async fn removing_execution_permission_after_enqueue_prevents_engine_execution() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = service(&ctx, &actor, query.clone());
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "revoke-before-execute".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    ctx.user_repo.remove_roles(&actor.sub).await.unwrap();
    assert!(service.run_once().await.unwrap());
    let actual = ctx
        .product_job_repo
        .get(&dts_backend::repo::product_job::ProductJobLookup {
            tenant_id: actor.tenant_id,
            job_id: job.id,
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert!(actual.result.is_none());
    assert!(query.calls.lock().unwrap().is_empty());
    assert!(
        actual
            .nodes
            .iter()
            .all(|node| node.status == ProductJobNodeStatus::Skipped)
    );
}

#[actix_web::test]
async fn cancelled_queued_jobs_do_not_run_and_oversized_real_results_fail_without_truncation() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = service(&ctx, &actor, query.clone());
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "cancel-before-claim".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    let cancelled = service.cancel(&actor, &job.id).await.unwrap();
    assert_eq!(cancelled.status, ProductJobStatus::Cancelled);
    assert!(!service.run_once().await.unwrap());
    assert!(query.calls.lock().unwrap().is_empty());
    query.oversized.store(true, Ordering::SeqCst);
    let retry = service
        .retry(
            &actor,
            &job.id,
            RetryRequest {
                idempotency_key: "retry-with-bounded-result".into(),
            },
        )
        .await
        .unwrap()
        .job;
    service.run_once().await.unwrap();
    let actual = service.get(&actor, &retry.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert_eq!(actual.nodes[0].status, ProductJobNodeStatus::Failed);
    assert!(actual.nodes[0].result.is_none());
    assert!(actual.nodes[0].error.as_ref().unwrap().contains("存储限额"));
}

#[actix_web::test]
async fn a_read_permission_does_not_grant_execution_and_foreign_job_lookup_is_not_found() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let service = service(&ctx, &actor, Arc::new(ObservedQuery::default()));
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "allowed-run".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    ctx.user_repo
        .create(&CreateUserParams {
            id: "job-reader".into(),
            tenant_id: actor.tenant_id.clone(),
            username: "job-reader".into(),
            email: "job-reader@test".into(),
            password_hash: "unused-fixture-hash".into(),
            display_name: None,
        })
        .await
        .unwrap();
    ctx.role_repo
        .create_with_permissions(&CreateRoleParams {
            id: "job-read-role".into(),
            tenant_id: actor.tenant_id.clone(),
            name: "job_reader".into(),
            description: "Only development read".into(),
            is_system: false,
            permissions: vec!["development:read".into()],
        })
        .await
        .unwrap();
    ctx.user_repo
        .assign_role("job-reader", "job-read-role")
        .await
        .unwrap();
    let reader_token = common::test_token(&ctx, "job-reader").await;
    let reader = ctx.auth.authenticate(&reader_token).await.unwrap();
    service.run_once().await.unwrap();
    let restricted = service.get(&reader, &job.id).await.unwrap();
    assert_eq!(restricted.id, job.id);
    assert_eq!(restricted.status, ProductJobStatus::Succeeded);
    assert_eq!(restricted.result_access, ResultAccess::Restricted);
    assert!(restricted.result.is_none());
    assert!(restricted.nodes[0].result.is_none());
    let serialized = serde_json::to_value(restricted).unwrap();
    assert_eq!(serialized["nodes"][0]["result_access"], "restricted");
    assert_eq!(serialized["parameters"], json!({}));
    assert_eq!(serialized["definition_snapshot"], json!({}));
    assert!(matches!(
        service
            .enqueue(
                &reader,
                "sql_document",
                &source.id,
                RunRequest {
                    expected_version: version,
                    idempotency_key: "forbidden-run".into(),
                    parameters: json!({})
                }
            )
            .await,
        Err(AppError::Forbidden(_))
    ));
    let other = dts_backend::types::CreateTenantParams {
        id: "other-job-tenant".into(),
        name: "Other".into(),
        slug: "other-job-tenant".into(),
    };
    ctx.tenant_repo.create(&other).await.unwrap();
    ctx.user_repo
        .create(&CreateUserParams {
            id: "other-job-reader".into(),
            tenant_id: other.id.clone(),
            username: "other-job-reader".into(),
            email: "other-job-reader@test".into(),
            password_hash: "unused-fixture-hash".into(),
            display_name: None,
        })
        .await
        .unwrap();
    let other_token = common::test_token(&ctx, "other-job-reader").await;
    let foreign = ctx.auth.authenticate(&other_token).await.unwrap();
    assert!(matches!(
        service.get(&foreign, &job.id).await,
        Err(AppError::NotFound(_))
    ));
}

#[derive(Default)]
struct BlockingQuery {
    entered: tokio::sync::Notify,
    dropped: Arc<AtomicBool>,
}

struct QueryDropGuard(Arc<AtomicBool>);
impl Drop for QueryDropGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for BlockingQuery {
    async fn execute(&self, _: &str) -> Result<QueryResult, AppError> {
        let _guard = QueryDropGuard(self.dropped.clone());
        self.entered.notify_one();
        std::future::pending().await
    }
}

#[actix_web::test]
async fn cancelling_a_running_query_drops_the_query_and_records_actual_cancelled_states() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(BlockingQuery::default());
    let service = bound_service(&ctx, &actor, query.clone());
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "cancel-during-query".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    let runner = service.clone();
    let task = tokio::spawn(async move { runner.run_once().await });
    tokio::time::timeout(Duration::from_secs(10), query.entered.notified())
        .await
        .unwrap();
    let requested = service.cancel(&actor, &job.id).await.unwrap();
    assert_eq!(requested.status, ProductJobStatus::Running);
    assert!(requested.cancel_requested);
    assert!(
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    );
    let actual = service.get(&actor, &job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Cancelled);
    assert_eq!(actual.nodes[0].status, ProductJobNodeStatus::Cancelled);
    assert!(actual.result.is_none());
    assert!(actual.nodes[0].result.is_none());
    assert!(query.dropped.load(Ordering::SeqCst));
}

#[actix_web::test]
async fn an_unconfigured_query_authorization_service_cannot_dispatch_user_sql() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(ObservedQuery::default());
    let service = ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query.clone(),
        QueryAccess {
            tenant_id: Some(actor.tenant_id.clone()),
        },
    );
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "unconfigured-access-guard".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    service.run_once().await.unwrap();
    let actual = service.get(&actor, &job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert_eq!(actual.nodes[0].status, ProductJobNodeStatus::Failed);
    assert!(actual.result.is_none());
    assert!(actual.nodes[0].result.is_none());
    assert!(query.calls.lock().unwrap().is_empty());
}

#[actix_web::test]
async fn withdrawing_data_access_during_execution_discards_results_and_records_denial() {
    let ctx = common::setup_test_backend().await.unwrap();
    let (actor, _) = identity(&ctx).await;
    let query = Arc::new(BlockingQuery::default());
    let service = bound_service(&ctx, &actor, query.clone());
    let source = source(&ctx, &actor, "sql_document", json!({"sql":"SELECT 1"})).await;
    let version = publish(&service, &actor, &source).await;
    let job = service
        .enqueue(
            &actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: version,
                idempotency_key: "withdraw-data-access-during-query".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap()
        .job;
    let runner = service.clone();
    let task = tokio::spawn(async move { runner.run_once().await });
    tokio::time::timeout(Duration::from_secs(10), query.entered.notified())
        .await
        .unwrap();
    let role = ctx
        .role_repo
        .list_all(Some(&actor.tenant_id))
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.name == "tenant_admin")
        .unwrap();
    let permissions = role
        .permissions
        .into_iter()
        .filter(|permission| permission != "data_security:all_data")
        .collect();
    ctx.role_repo
        .update(
            &role.id,
            &dts_backend::types::UpdateRoleParams {
                name: None,
                permissions: Some(permissions),
            },
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    );
    let actual = service.get(&actor, &job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert!(actual.result.is_none());
    assert!(actual.nodes[0].result.is_none());
    assert!(query.dropped.load(Ordering::SeqCst));
    let audits = ctx
        .data_access_repo
        .list_audit(&dts_backend::repo::data_access::DataAccessAuditFilter {
            tenant_id: actor.tenant_id,
            actor_id: actor.sub,
            limit: 50,
            offset: 0,
        })
        .await
        .unwrap();
    assert!(audits.data.iter().any(|audit| audit.outcome == "denied"));
    assert!(!audits.data.iter().any(|audit| audit.outcome == "succeeded"));
}
