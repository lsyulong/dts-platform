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
        data_services::{DataApiRunRequest, DataApiService},
        metadata::{MetadataRunRequest, MetadataService},
        product_jobs::{ProductJobService, ReadOnlyQueryExecutor, ResultAccess},
        query_svc::{ColumnMeta, QueryResult},
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

// Deliberate deterministic observations for SQLite control-plane verification;
// this executor does not pretend to be a live SwanLake POC.
#[derive(Default)]
struct Observations {
    calls: Mutex<Vec<String>>,
    mode: AtomicUsize,
    paused: AtomicBool,
    started: Notify,
    released: Notify,
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
        if self.paused.load(Ordering::SeqCst) {
            self.started.notify_one();
            self.released.notified().await;
        }
        let mode = self.mode.load(Ordering::SeqCst);
        let rows = if mode == 1 {
            vec![json!({"region":"overflow","amount":"1.00"}); 3]
        } else {
            vec![json!({"region":"华东","amount":"125.50"})]
        };
        Ok(QueryResult {
            columns: vec![
                ColumnMeta {
                    name: "region".into(),
                    data_type: "Utf8".into(),
                    is_numeric: false,
                },
                ColumnMeta {
                    name: "amount".into(),
                    data_type: if mode == 2 {
                        "Utf8"
                    } else {
                        "Decimal128(18, 2)"
                    }
                    .into(),
                    is_numeric: mode != 2,
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
                name: "真实数据API合同测试".into(),
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
    api: DataApiService,
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
    let collection = resource(
        &ctx,
        &actor,
        "metadata_collection",
        json!({"source_type":"swanlake"}),
    )
    .await;
    let collected = collector
        .enqueue(
            &actor,
            &collection.id,
            MetadataRunRequest {
                expected_version: collection.version,
                idempotency_key: "api-registry-fixture".into(),
            },
        )
        .await
        .unwrap();
    worker.run_once().await.unwrap();
    assert_eq!(
        worker.get(&actor, &collected.job.id).await.unwrap().status,
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
    let api = DataApiService::new(
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
        api,
        metadata,
    }
}

fn definition(metadata_id: &str) -> Value {
    json!({"metadata_object_id":metadata_id,"sql":"SELECT region, amount FROM lake.main.orders WHERE region=$area ORDER BY region",
        "parameters":[{"name":"area","data_type":"VARCHAR","required":true}],"response_schema":[],"row_limit":2,"rate_limit":60})
}

async fn published(f: &Fixture) -> ProductResource {
    let source = resource(&f.ctx, &f.actor, "data_api", definition(&f.metadata.id)).await;
    let validated = f
        .api
        .validate_saved(&f.actor, &source.id, source.version)
        .await
        .unwrap();
    assert_eq!(
        validated["response_schema"][1]["data_type"],
        "DECIMAL(18,2)"
    );
    let published = f
        .api
        .publish(&f.actor, &source.id, source.version)
        .await
        .unwrap();
    assert_eq!(published["executed"], false);
    assert_eq!(published["external_credentials_ready"], false);
    assert_eq!(published["rate_limit_enforced"], false);
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    serde_json::from_value(published["resource"].clone()).unwrap()
}

async fn enqueue(f: &Fixture, source: &ProductResource, key: &str) -> String {
    f.api
        .enqueue(
            &f.actor,
            &source.id,
            DataApiRunRequest {
                expected_version: source.version,
                idempotency_key: key.into(),
                parameters: json!({"area":"华东"}),
            },
        )
        .await
        .unwrap()
        .job
        .id
}

#[actix_web::test]
async fn real_publication_freezes_schema_and_console_job_returns_only_observed_rows() {
    let f = fixture().await;
    let source = published(&f).await;
    assert_eq!(
        source.properties["publication"]["source_version"],
        source.version
    );
    assert_eq!(source.properties["response_schema"][0]["nullable"], false);
    let submitted = f
        .api
        .enqueue(
            &f.actor,
            &source.id,
            DataApiRunRequest {
                expected_version: source.version,
                idempotency_key: "api-first-run".into(),
                parameters: json!({"area":"O'Reilly' OR TRUE --"}),
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
    assert_eq!(result["rows"], json!([{"region":"华东","amount":"125.50"}]));
    assert_eq!(result["row_count"], 1);
    assert_eq!(result["api_version"], source.version);
    assert!(result["privacy"]["signature"].as_str().unwrap().len() > 20);
    let calls = f.query.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].contains("'O''Reilly'' OR TRUE --'"));
    assert!(calls[1].ends_with("LIMIT 3"));
}

#[actix_web::test]
async fn sentinel_overflow_and_actual_schema_drift_fail_without_persisting_partial_rows() {
    for mode in [1, 2] {
        let f = fixture().await;
        let source = published(&f).await;
        f.query.mode.store(mode, Ordering::SeqCst);
        let id = enqueue(&f, &source, "api-invalid-output").await;
        f.worker.run_once().await.unwrap();
        let actual = f.worker.get(&f.actor, &id).await.unwrap();
        assert_eq!(actual.status, ProductJobStatus::Failed);
        assert!(actual.result.is_none());
        assert!(actual.nodes[0].result.is_none());
        assert_eq!(f.query.calls.lock().unwrap().len(), 2);
    }
}

#[actix_web::test]
async fn withdrawal_blocks_queued_execution_and_historical_result_release() {
    let f = fixture().await;
    let source = published(&f).await;
    let queued = enqueue(&f, &source, "api-withdraw-queued").await;
    f.api
        .withdraw(&f.actor, &source.id, source.version)
        .await
        .unwrap();
    f.worker.run_once().await.unwrap();
    assert_eq!(
        f.worker.get(&f.actor, &queued).await.unwrap().status,
        ProductJobStatus::Failed
    );
    assert_eq!(f.query.calls.lock().unwrap().len(), 1);
    let f = fixture().await;
    let source = published(&f).await;
    let id = enqueue(&f, &source, "api-withdraw-history").await;
    f.worker.run_once().await.unwrap();
    assert_eq!(
        f.worker.get(&f.actor, &id).await.unwrap().result_access,
        ResultAccess::Authorized
    );
    f.api
        .withdraw(&f.actor, &source.id, source.version)
        .await
        .unwrap();
    let restricted = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(restricted.status, ProductJobStatus::Succeeded);
    assert_eq!(restricted.result_access, ResultAccess::Restricted);
    assert!(restricted.nodes[0].result.is_none());
    let raw = f
        .ctx
        .product_job_repo
        .get(&ProductJobLookup {
            tenant_id: f.actor.tenant_id.clone(),
            job_id: id,
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert!(raw.nodes[0].result.is_some());
}

#[actix_web::test]
async fn withdrawal_during_query_does_not_commit_or_release_returned_rows() {
    let f = fixture().await;
    let source = published(&f).await;
    let id = enqueue(&f, &source, "api-withdraw-running").await;
    f.query.paused.store(true, Ordering::SeqCst);
    let (ran, ()) = tokio::join!(f.worker.run_once(), async {
        f.query.started.notified().await;
        f.api
            .withdraw(&f.actor, &source.id, source.version)
            .await
            .unwrap();
        f.query.released.notify_one();
    });
    ran.unwrap();
    let actual = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert!(actual.nodes[0].result.is_none());
    assert_eq!(f.query.calls.lock().unwrap().len(), 2);
}

#[actix_web::test]
async fn forged_registry_is_rejected_and_opaque_synchronous_call_rechecks_withdrawal() {
    let f = fixture().await;
    let forged = resource(
        &f.ctx,
        &f.actor,
        "metadata_object",
        f.metadata.properties.clone(),
    )
    .await;
    let unbound = resource(&f.ctx, &f.actor, "data_api", definition(&forged.id)).await;
    assert!(
        f.api
            .publish(&f.actor, &unbound.id, unbound.version)
            .await
            .is_err()
    );
    let source = published(&f).await;
    assert!(
        f.api
            .prepare_invocation(&f.actor, &source.id, source.version, &json!({"area":7}))
            .await
            .is_err()
    );
    let prepared = f
        .api
        .prepare_invocation(
            &f.actor,
            &source.id,
            source.version,
            &json!({"area":"华东"}),
        )
        .await
        .unwrap();
    assert!(prepared.sql().ends_with("LIMIT 3"));
    f.api
        .reauthorize_invocation(&f.actor, &prepared)
        .await
        .unwrap();
    let actual = f.query.execute(prepared.sql()).await.unwrap();
    let authorization = QueryAuthorizationService::new(
        f.ctx.data_access_repo.clone(),
        f.ctx.metadata_catalog_repo.clone(),
        QueryAccess {
            tenant_id: Some(f.actor.tenant_id.clone()),
        },
    );
    let query = authorization
        .authorize(&f.actor, prepared.sql())
        .await
        .unwrap();
    let privacy = masking::service(&f.ctx);
    let plan = privacy.prepare_output(&f.actor, &query).await.unwrap();
    let other_query = authorization.authorize(&f.actor, "SELECT 1").await.unwrap();
    let other_plan = privacy
        .prepare_output(&f.actor, &other_query)
        .await
        .unwrap();
    assert!(matches!(
        f.api.protect_invocation(
            &prepared,
            QueryResult {
                columns: vec![],
                rows: vec![],
                row_count: 0,
                suggested_chart: None,
            },
            &privacy,
            &other_plan,
        ),
        Err(AppError::Forbidden(_))
    ));
    let protected = f
        .api
        .protect_invocation(&prepared, actual, &privacy, &plan)
        .unwrap();
    let mapped = f.api.map_invocation(&prepared, protected).unwrap();
    assert_eq!(mapped["row_count"], 1);
    assert!(mapped.get("rate_limit_enforced").is_none());
    f.api
        .withdraw(&f.actor, &source.id, source.version)
        .await
        .unwrap();
    assert!(
        f.api
            .reauthorize_invocation(&f.actor, &prepared)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn numeric_mask_has_actual_effective_schema_and_cannot_hide_raw_physical_schema_drift() {
    let f = fixture().await;
    let source = published(&f).await;
    let privacy = masking::service(&f.ctx);
    let rule = resource(
        &f.ctx,
        &f.actor,
        "masking_rule",
        json!({"object_id":f.metadata.id,"column_name":"amount","method":"hash","parameters":{}}),
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
    let id = enqueue(&f, &source, "api-effective-mask").await;
    f.worker.run_once().await.unwrap();
    let first = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(first.status, ProductJobStatus::Succeeded);
    assert_eq!(first.result_access, ResultAccess::Authorized);
    let result = first.nodes[0].result.as_ref().unwrap();
    assert_eq!(
        result["source_response_schema"][1]["data_type"],
        "DECIMAL(18,2)"
    );
    assert_eq!(
        result["effective_response_schema"][1]["data_type"],
        "VARCHAR"
    );
    assert_eq!(result["effective_response_schema"][1]["nullable"], true);
    assert_eq!(result["columns"][1]["is_numeric"], false);
    assert_eq!(result["mask_applied"], true);
    assert!(
        result["rows"][0]["amount"]
            .as_str()
            .unwrap()
            .starts_with("hmac:v1:")
    );
    let second = f.worker.get(&f.actor, &id).await.unwrap();
    assert_eq!(
        second.nodes[0].result.as_ref().unwrap()["rows"],
        result["rows"]
    );
    f.query.mode.store(2, Ordering::SeqCst);
    let bad = enqueue(&f, &source, "api-masked-schema-drift").await;
    f.worker.run_once().await.unwrap();
    let rejected = f.worker.get(&f.actor, &bad).await.unwrap();
    assert_eq!(rejected.status, ProductJobStatus::Failed);
    assert!(rejected.nodes[0].result.is_none());
}
