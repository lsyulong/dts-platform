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
        bi_report::{BiReportService, BiRunRequest},
        data_access::QueryAuthorizationService,
        metadata::{MetadataRunRequest, MetadataService},
        product_jobs::{ProductJobService, ReadOnlyQueryExecutor, ResultAccess, RunRequest},
        query_svc::{ColumnMeta, QueryResult},
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Explicit deterministic engine observations; this tests production control
/// plane services and SQLite, and is not a live SwanLake POC.
#[derive(Default)]
struct Observations {
    calls: Mutex<Vec<String>>,
    overflow: AtomicBool,
}

#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for Observations {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        self.calls.lock().unwrap().push(sql.into());
        if sql.contains("information_schema") {
            let rows = vec![
                json!({"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE","column_name":"region","data_type":"VARCHAR","is_nullable":"NO","ordinal_position":1}),
                json!({"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE","column_name":"amount","data_type":"DECIMAL(18,2)","is_nullable":"YES","ordinal_position":2}),
            ];
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
                row_count: rows.len(),
                rows,
                suggested_chart: None,
            });
        }
        let rows = if self.overflow.load(Ordering::SeqCst) {
            vec![json!({"region":"overflow","total_amount":1}); 3]
        } else {
            vec![
                json!({"region":"华东","total_amount":125.5}),
                json!({"region":"华南","total_amount":80}),
            ]
        };
        Ok(QueryResult {
            columns: vec![
                ColumnMeta {
                    name: "region".into(),
                    data_type: "Utf8".into(),
                    is_numeric: false,
                },
                ColumnMeta {
                    name: "total_amount".into(),
                    data_type: "Decimal128".into(),
                    is_numeric: true,
                },
            ],
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
                name: "真实报表合同测试".into(),
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
    bi: BiReportService,
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
        .with_job_repo(ctx.product_job_repo.clone()),
    );
    let worker = ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query.clone(),
        access.clone(),
    )
    .with_metadata_repo(ctx.metadata_catalog_repo.clone())
    .with_query_authorization(guard)
    .with_masking(masking::service(&ctx));
    let collector = MetadataService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.auth.clone(),
        access.clone(),
    );
    let source = resource(
        &ctx,
        &actor,
        "metadata_collection",
        json!({"source_type":"swanlake"}),
    )
    .await;
    let job = collector
        .enqueue(
            &actor,
            &source.id,
            MetadataRunRequest {
                expected_version: source.version,
                idempotency_key: "bi-real-registry-fixture".into(),
            },
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(
        worker.get(&actor, &job.job.id).await.unwrap().status,
        ProductJobStatus::Succeeded
    );
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
    let bi = BiReportService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.auth.clone(),
        access,
    );
    Fixture {
        ctx,
        actor,
        query,
        worker,
        bi,
        metadata,
    }
}

fn definition(metadata_id: &str) -> Value {
    json!({"metadata_object_id":metadata_id,"dimensions":["region"],"measures":[{"name":"total_amount","expression":"amount","aggregate":"sum"}],"row_limit":2,
        "chart_config":{"type":"bar","title":"实际订单聚合","x_field":"region","y_field":"total_amount","legend":true}})
}

async fn published(f: &Fixture) -> (ProductResource, i64) {
    let source = resource(&f.ctx, &f.actor, "bi_report", definition(&f.metadata.id)).await;
    let result =
        f.bi.publish(&f.actor, &source.id, source.version)
            .await
            .unwrap();
    assert_eq!(result["executed"], false);
    assert_eq!(result["schedule_registered"], false);
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    (source, result["resource"]["version"].as_i64().unwrap())
}

#[actix_web::test]
async fn bi_uses_real_collected_metadata_and_persists_observed_aggregate_rows() {
    let f = fixture().await;
    let (source, version) = published(&f).await;
    let submitted =
        f.bi.enqueue(
            &f.actor,
            &source.id,
            BiRunRequest {
                expected_version: version,
                idempotency_key: "bi-first-run".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(submitted.job.status, ProductJobStatus::Queued);
    assert!(submitted.job.result.is_none());
    f.worker.run_once().await.unwrap();
    let actual = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Succeeded);
    assert_eq!(actual.result_access, ResultAccess::Authorized);
    let result = actual.nodes[0].result.as_ref().unwrap();
    assert_eq!(result["row_count"], 2);
    assert_eq!(result["complete"], true);
    assert_eq!(
        result["rows"][0],
        json!({"region":"华东","total_amount":125.5})
    );
    assert_eq!(result["chart_config"]["type"], "bar");
    let calls = f.query.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1],
        "SELECT \"region\", SUM(\"amount\") AS \"total_amount\" FROM \"lake\".\"main\".\"orders\" GROUP BY \"region\" ORDER BY \"region\" LIMIT 3"
    );
}

#[actix_web::test]
async fn aggregation_overflow_is_a_real_failed_node_without_truncated_success_rows() {
    let f = fixture().await;
    let (source, version) = published(&f).await;
    f.query.overflow.store(true, Ordering::SeqCst);
    let submitted =
        f.bi.enqueue(
            &f.actor,
            &source.id,
            BiRunRequest {
                expected_version: version,
                idempotency_key: "bi-overflow".into(),
            },
        )
        .await
        .unwrap();
    f.worker.run_once().await.unwrap();
    let actual = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert!(actual.result.is_none());
    assert!(actual.nodes[0].result.is_none());
    assert_eq!(f.query.calls.lock().unwrap().len(), 2);
}

#[actix_web::test]
async fn forged_registry_and_free_sql_are_rejected_and_expired_reader_access_hides_saved_rows() {
    let f = fixture().await;
    let forged = resource(
        &f.ctx,
        &f.actor,
        "metadata_object",
        f.metadata.properties.clone(),
    )
    .await;
    let unbound = resource(&f.ctx, &f.actor, "bi_report", definition(&forged.id)).await;
    assert!(
        f.bi.validate_saved(&f.actor, &unbound.id, unbound.version)
            .await
            .is_err()
    );
    let mut sql = definition(&f.metadata.id);
    sql["sql"] = json!("SELECT * FROM secrets");
    let sqlsource = resource(&f.ctx, &f.actor, "bi_report", sql).await;
    assert!(
        f.bi.publish(&f.actor, &sqlsource.id, sqlsource.version)
            .await
            .is_err()
    );
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    let (source, version) = published(&f).await;
    let submitted =
        f.bi.enqueue(
            &f.actor,
            &source.id,
            BiRunRequest {
                expected_version: version,
                idempotency_key: "bi-history".into(),
            },
        )
        .await
        .unwrap();
    f.worker.run_once().await.unwrap();
    assert_eq!(
        f.worker
            .get(&f.actor, &submitted.job.id)
            .await
            .unwrap()
            .result_access,
        ResultAccess::Authorized
    );
    let role = f
        .ctx
        .role_repo
        .list_all(Some(&f.actor.tenant_id))
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.name == "tenant_admin")
        .unwrap();
    f.ctx
        .role_repo
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
    let restricted = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    assert_eq!(restricted.status, ProductJobStatus::Succeeded);
    assert_eq!(restricted.result_access, ResultAccess::Restricted);
    assert!(restricted.nodes[0].result.is_none());
    let raw = f
        .ctx
        .product_job_repo
        .get(&ProductJobLookup {
            tenant_id: f.actor.tenant_id.clone(),
            job_id: submitted.job.id,
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert!(raw.nodes[0].result.is_some());
}

#[actix_web::test]
async fn worker_stores_masked_rows_and_history_does_not_hash_again_or_release_changed_policy() {
    let f = fixture().await;
    let privacy = masking::service(&f.ctx);
    let rule = resource(
        &f.ctx,
        &f.actor,
        "masking_rule",
        json!({"object_id":f.metadata.id,
        "column_name":"region","method":"hash","parameters":{}}),
    )
    .await;
    let rule = privacy
        .publish(&f.actor, "masking_rule", &rule.id, rule.version)
        .await
        .unwrap();
    let group = resource(
        &f.ctx,
        &f.actor,
        "user_group",
        json!({"member_ids":[f.actor.sub],"masking_rule_id":rule.id}),
    )
    .await;
    privacy
        .publish(&f.actor, "user_group", &group.id, group.version)
        .await
        .unwrap();
    let source = resource(
        &f.ctx,
        &f.actor,
        "sql_document",
        json!({"sql":"SELECT region, amount AS total_amount FROM lake.main.orders"}),
    )
    .await;
    let published = f
        .worker
        .publish(&f.actor, "sql_document", &source.id, source.version)
        .await
        .unwrap();
    let submitted = f
        .worker
        .enqueue(
            &f.actor,
            "sql_document",
            &source.id,
            RunRequest {
                expected_version: published["resource"]["version"].as_i64().unwrap(),
                idempotency_key: "stored-mask-pipeline".into(),
                parameters: json!({}),
            },
        )
        .await
        .unwrap();
    f.worker.run_once().await.unwrap();
    let lookup = ProductJobLookup {
        tenant_id: f.actor.tenant_id.clone(),
        job_id: submitted.job.id.clone(),
        actor_id: None,
    };
    let raw = f.ctx.product_job_repo.get(&lookup).await.unwrap().unwrap();
    assert_eq!(raw.status, ProductJobStatus::Succeeded);
    let persisted = raw.nodes[0].result.as_ref().unwrap();
    let masked = persisted["rows"][0]["region"].as_str().unwrap();
    assert!(masked.starts_with("hmac:v1:"));
    assert!(!serde_json::to_string(persisted).unwrap().contains("华东"));
    assert_eq!(persisted["columns"][0]["is_numeric"], false);
    assert!(persisted["privacy"]["signature"].as_str().unwrap().len() > 20);
    let first = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    let second = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    assert_eq!(first.result_access, ResultAccess::Authorized);
    assert_eq!(
        first.nodes[0].result.as_ref().unwrap()["rows"][0]["region"],
        masked
    );
    assert_eq!(
        second.nodes[0].result.as_ref().unwrap()["rows"][0]["region"],
        masked
    );
    privacy
        .withdraw(&f.actor, "masking_rule", &rule.id, rule.version)
        .await
        .unwrap();
    let changed = f.worker.get(&f.actor, &submitted.job.id).await.unwrap();
    assert_eq!(changed.status, ProductJobStatus::Succeeded);
    assert_eq!(changed.result_access, ResultAccess::Restricted);
    assert_eq!(changed.result_access_reason, Some("privacy_policy_changed"));
    assert!(changed.nodes[0].result.is_none());
    let retained = f.ctx.product_job_repo.get(&lookup).await.unwrap().unwrap();
    assert_eq!(
        retained.nodes[0].result.as_ref().unwrap()["rows"][0]["region"],
        masked
    );
}
