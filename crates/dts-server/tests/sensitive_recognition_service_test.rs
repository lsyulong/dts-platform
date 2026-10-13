// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
mod common;
#[path = "product_job_service_support/masking.rs"]
mod masking;
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ResourceWrite,
    repo::product_job::{ProductJobLookup, ProductJobStatus},
};
use dts_server::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::QueryAuthorizationService,
        metadata::{MetadataRunRequest, MetadataService},
        product_jobs::{ProductJobService, ReadOnlyQueryExecutor, ResultAccess},
        query_svc::{ColumnMeta, QueryResult},
        sensitive_recognition::*,
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

// Explicit observations for control-plane integration; no SwanLake POC is run.
#[derive(Default)]
struct Observations {
    calls: Mutex<Vec<String>>,
    paused: AtomicBool,
    overflow: AtomicBool,
    started: Notify,
    released: Notify,
}
#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for Observations {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        self.calls.lock().unwrap().push(sql.into());
        if sql.contains("information_schema") {
            let row = json!({"table_catalog":"lake","schema_name":"main","table_name":"customers","table_type":"BASE TABLE","column_name":"email","data_type":"VARCHAR","is_nullable":"YES","ordinal_position":1});
            return Ok(QueryResult {
                columns: [
                    "table_catalog",
                    "schema_name",
                    "table_name",
                    "table_type",
                    "column_name",
                    "data_type",
                    "is_nullable",
                    "ordinal_position",
                ]
                .into_iter()
                .map(|name| ColumnMeta {
                    name: name.into(),
                    data_type: "fixture".into(),
                    is_numeric: false,
                })
                .collect(),
                rows: vec![row],
                row_count: 1,
                suggested_chart: None,
            });
        }
        if self.paused.load(Ordering::SeqCst) {
            self.started.notify_one();
            self.released.notified().await;
        }
        let rows = if self.overflow.load(Ordering::SeqCst) {
            vec![json!({"email":"oversized@company.com"}); 4]
        } else {
            vec![
                json!({"email":"private@company.com"}),
                json!({"email":"plain text"}),
                json!({"email":null}),
            ]
        };
        Ok(QueryResult {
            columns: vec![ColumnMeta {
                name: "email".into(),
                data_type: "Utf8".into(),
                is_numeric: false,
            }],
            row_count: rows.len(),
            rows,
            suggested_chart: None,
        })
    }
}
async fn resource(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    kind: &str,
    properties: Value,
) -> ProductResource {
    let id = uuid::Uuid::new_v4().to_string();
    ctx.product_resource_repo
        .create(&CreateResourceParams {
            id: id.clone(),
            tenant_id: actor.tenant_id.clone(),
            kind: kind.into(),
            actor_id: actor.sub.clone(),
            fields: ResourceWrite {
                code: id,
                name: "敏感识别实际合同测试".into(),
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
struct Fixture {
    ctx: common::TestBackend,
    actor: AuthIdentity,
    query: Arc<Observations>,
    worker: ProductJobService,
    service: SensitiveRecognitionService,
    metadata: ProductResource,
}
async fn fixture() -> Fixture {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let query = Arc::new(Observations::default());
    let access = QueryAccess {
        tenant_id: Some(actor.tenant_id.clone()),
    };
    let guard = Arc::new(
        QueryAuthorizationService::new(
            ctx.data_access_repo.clone(),
            ctx.metadata_catalog_repo.clone(),
            access.clone(),
        )
        .with_job_repo(ctx.product_job_repo.clone())
        .with_resource_repo(ctx.product_resource_repo.clone()),
    );
    let worker = ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query.clone(),
        access.clone(),
    )
    .with_metadata_repo(ctx.metadata_catalog_repo.clone())
    .with_sensitive_findings_repo(ctx.sensitive_finding_repo.clone())
    .with_query_authorization(guard.clone())
    .with_masking(masking::service(&ctx));
    let collector = MetadataService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.auth.clone(),
        access.clone(),
    );
    let collection = resource(
        &ctx,
        &actor,
        "metadata_collection",
        json!({"source_type":"swanlake"}),
    )
    .await;
    collector
        .enqueue(
            &actor,
            &collection.id,
            MetadataRunRequest {
                expected_version: collection.version,
                idempotency_key: "sensitive-actual-registry-fixture".into(),
            },
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    let metadata = ctx
        .product_resource_repo
        .list(&ProductResourceFilter {
            tenant_id: actor.tenant_id.clone(),
            kind: "metadata_object".into(),
            q: None,
            status: None,
            created_by: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap()
        .data
        .remove(0);
    let service = SensitiveRecognitionService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.sensitive_finding_repo.clone(),
        ctx.auth.clone(),
        access,
    )
    .with_query_authorization(guard);
    Fixture {
        ctx,
        actor,
        query,
        worker,
        service,
        metadata,
    }
}
async fn published(f: &Fixture) -> ProductResource {
    let class = resource(
        &f.ctx,
        &f.actor,
        "data_classification",
        json!({"rank":2,"handling_policy":"需人工确认"}),
    )
    .await;
    let class = f
        .service
        .publish(&f.actor, "data_classification", &class.id, class.version)
        .await
        .unwrap();
    let class: ProductResource = serde_json::from_value(class["resource"].clone()).unwrap();
    let rule=resource(&f.ctx,&f.actor,"recognition_rule",json!({"classification_id":class.id,"pattern":"^[^@]+@[^@]+[.]com$","match_target":"sample_value","source_types":["text"],"sample_limit":3})).await;
    let rule = f
        .service
        .publish(&f.actor, "recognition_rule", &rule.id, rule.version)
        .await
        .unwrap();
    let rule: ProductResource = serde_json::from_value(rule["resource"].clone()).unwrap();
    let task = resource(
        &f.ctx,
        &f.actor,
        "recognition_task",
        json!({"rule_id":rule.id,"object_id":f.metadata.id,"sample_limit":3}),
    )
    .await;
    let task = f
        .service
        .publish(&f.actor, "recognition_task", &task.id, task.version)
        .await
        .unwrap();
    assert_eq!(task["executed"], false);
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    serde_json::from_value(task["resource"].clone()).unwrap()
}
async fn enqueue(f: &Fixture, task: &ProductResource, key: &str) -> String {
    f.service
        .enqueue(
            &f.actor,
            &task.id,
            RecognitionRunRequest {
                expected_version: task.version,
                idempotency_key: key.into(),
            },
        )
        .await
        .unwrap()
        .job
        .id
}
async fn raw_job(f: &Fixture, id: &str) -> dts_backend::repo::product_job::ProductJob {
    f.ctx
        .product_job_repo
        .get(&ProductJobLookup {
            tenant_id: f.actor.tenant_id.clone(),
            job_id: id.into(),
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap()
}

#[actix_web::test]
async fn real_worker_observation_produces_only_counts_and_atomic_findings() {
    let f = fixture().await;
    let task = published(&f).await;
    let id = enqueue(&f, &task, "sensitive-run").await;
    f.worker.run_once().await.unwrap();
    let job = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(job.status, ProductJobStatus::Succeeded);
    assert_eq!(job.result_access, ResultAccess::Authorized);
    let result = job.nodes[0].result.as_ref().unwrap();
    assert_eq!(result["producer_committed"], true);
    assert_eq!(result["sampled_rows"], 3);
    assert_eq!(result["columns"][0]["matched_values"], 1);
    assert_eq!(result["evaluation_scope"], "sample");
    let stored = raw_job(&f, &id).await;
    let encoded = serde_json::to_string(&stored).unwrap();
    assert!(!encoded.contains("private@company.com"));
    assert!(!encoded.contains("plain text"));
    assert!(
        stored.nodes[0]
            .result
            .as_ref()
            .unwrap()
            .get("rows")
            .is_none()
    );
    let findings = f
        .service
        .findings(
            &f.actor,
            RecognitionListQuery {
                task_id: Some(task.id.clone()),
                status: None,
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(findings.total, 1);
    let found = &findings.data[0];
    assert_eq!(found.evidence.matched_values, 1);
    let reply = f
        .service
        .review(
            &f.actor,
            &found.id,
            true,
            RecognitionReviewRequest {
                expected_version: found.version,
                reason: "人工确认业务字段分类建议".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["status"], "confirmed");
    assert_eq!(reply["classification_enforced"], false);
    assert!(reply.get("evidence").is_none());
    f.service
        .withdraw(&f.actor, "recognition_task", &task.id, task.version)
        .await
        .unwrap();
    let protected = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(protected.status, ProductJobStatus::Succeeded);
    assert_eq!(protected.result_access, ResultAccess::Restricted);
    assert!(protected.nodes[0].result.is_none());
    assert!(f.service.finding(&f.actor, &found.id).await.is_err());
    assert!(raw_job(&f, &id).await.nodes[0].result.is_some());
}
#[actix_web::test]
async fn cancellation_during_actual_query_prevents_all_findings() {
    let f = fixture().await;
    let task = published(&f).await;
    let id = enqueue(&f, &task, "sensitive-cancel").await;
    f.query.paused.store(true, Ordering::SeqCst);
    let (ran, ()) = tokio::join!(f.worker.run_once(), async {
        f.query.started.notified().await;
        f.worker.cancel(&f.actor, &id).await.unwrap();
        f.query.released.notify_one();
    });
    ran.unwrap();
    assert_eq!(raw_job(&f, &id).await.status, ProductJobStatus::Cancelled);
    let page = f
        .service
        .findings(
            &f.actor,
            RecognitionListQuery {
                task_id: Some(task.id),
                status: None,
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 0);
}
#[actix_web::test]
async fn actual_sample_overflow_is_failed_without_partial_discoveries() {
    let f = fixture().await;
    let task = published(&f).await;
    let id = enqueue(&f, &task, "sensitive-overflow").await;
    f.query.overflow.store(true, Ordering::SeqCst);
    f.worker.run_once().await.unwrap();
    let job = raw_job(&f, &id).await;
    assert_eq!(job.status, ProductJobStatus::Failed);
    assert!(job.nodes[0].result.is_none());
    assert!(job.result.is_none());
    assert_eq!(
        f.service
            .findings(
                &f.actor,
                RecognitionListQuery {
                    task_id: Some(task.id),
                    status: None,
                    limit: None,
                    offset: None
                }
            )
            .await
            .unwrap()
            .total,
        0
    );
}
#[actix_web::test]
async fn revoked_rule_fails_before_scan_and_forged_metadata_cannot_publish() {
    let f = fixture().await;
    let task = published(&f).await;
    let id = enqueue(&f, &task, "sensitive-stale-rule").await;
    let forged = resource(
        &f.ctx,
        &f.actor,
        "metadata_object",
        f.metadata.properties.clone(),
    )
    .await;
    let forged_task = resource(
        &f.ctx,
        &f.actor,
        "recognition_task",
        json!({"rule_id":task.properties["rule_id"],"object_id":forged.id,"sample_limit":3}),
    )
    .await;
    assert!(
        f.service
            .publish(
                &f.actor,
                "recognition_task",
                &forged_task.id,
                forged_task.version
            )
            .await
            .is_err()
    );
    let rule = &task.properties["publication"]["rule"];
    f.service
        .withdraw(
            &f.actor,
            "recognition_rule",
            rule["id"].as_str().unwrap(),
            rule["version"].as_i64().unwrap(),
        )
        .await
        .unwrap();
    f.worker.run_once().await.unwrap();
    assert_eq!(raw_job(&f, &id).await.status, ProductJobStatus::Failed);
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    assert!(
        f.ctx
            .metadata_catalog_repo
            .verified_binding(&f.actor.tenant_id, &forged.id, forged.version)
            .await
            .unwrap()
            .is_none()
    );
}
