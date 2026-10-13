// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ResourceWrite,
    repo::product_job::ProductJobStatus,
};
use dts_server::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        metadata::{MetadataRunRequest, MetadataService},
        product_jobs::{ProductJobService, ReadOnlyQueryExecutor},
        query_svc::{ColumnMeta, QueryResult},
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct SnapshotQuery {
    rows: Mutex<Vec<Value>>,
    calls: Mutex<Vec<String>>,
    fail: AtomicBool,
}
#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for SnapshotQuery {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        self.calls.lock().unwrap().push(sql.into());
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::BadRequest("controlled engine failure".into()));
        }
        let rows = self.rows.lock().unwrap().clone();
        Ok(QueryResult {
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
        })
    }
}
fn row(schema: &str, table: &str, column: &str, ordinal: u32) -> Value {
    json!({"table_catalog":"lake","schema_name":schema,"table_name":table,"table_type":"BASE TABLE","column_name":column,
        "data_type":"BIGINT","is_nullable":"NO","ordinal_position":ordinal})
}
async fn setup() -> (common::TestBackend, AuthIdentity) {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    (ctx, actor)
}
fn services(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    query: Arc<SnapshotQuery>,
) -> (MetadataService, ProductJobService) {
    let access = QueryAccess {
        tenant_id: Some(actor.tenant_id.clone()),
    };
    (
        MetadataService::new(
            ctx.product_resource_repo.clone(),
            ctx.product_job_repo.clone(),
            ctx.auth.clone(),
            access.clone(),
        ),
        ProductJobService::with_executor(
            ctx.product_job_repo.clone(),
            ctx.product_resource_repo.clone(),
            ctx.auth.clone(),
            query,
            access,
        )
        .with_metadata_repo(ctx.metadata_catalog_repo.clone()),
    )
}
async fn collection(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    properties: Value,
) -> ProductResource {
    let id = uuid::Uuid::new_v4().to_string();
    ctx.product_resource_repo
        .create(&CreateResourceParams {
            id: id.clone(),
            tenant_id: actor.tenant_id.clone(),
            kind: "metadata_collection".into(),
            actor_id: actor.sub.clone(),
            fields: ResourceWrite {
                code: id,
                name: "Metadata fixture".into(),
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
async fn objects(ctx: &common::TestBackend, actor: &AuthIdentity) -> Vec<ProductResource> {
    ctx.product_resource_repo
        .list(&ProductResourceFilter {
            tenant_id: actor.tenant_id.clone(),
            kind: "metadata_object".into(),
            q: None,
            status: None,
            created_by: None,
            limit: 100,
            offset: 0,
        })
        .await
        .unwrap()
        .data
}
async fn collect(
    collector: &MetadataService,
    worker: &ProductJobService,
    actor: &AuthIdentity,
    source: &ProductResource,
    key: &str,
) -> Value {
    let submission = collector
        .enqueue(
            actor,
            &source.id,
            MetadataRunRequest {
                expected_version: source.version,
                idempotency_key: key.into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(submission.job.status, ProductJobStatus::Queued);
    assert!(submission.job.result.is_none());
    assert!(worker.run_once().await.unwrap());
    let actual = worker.get(actor, &submission.job.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Succeeded);
    actual.result.clone().unwrap()
}

#[actix_web::test]
async fn actual_snapshot_reflection_is_idempotent_and_structural_changes_increment_only_changed_objects()
 {
    let (ctx, actor) = setup().await;
    let query = Arc::new(SnapshotQuery::default());
    *query.rows.lock().unwrap() = vec![
        row("main", "orders", "id", 1),
        row("main", "customers", "id", 1),
    ];
    let (collector, worker) = services(&ctx, &actor, query.clone());
    let source = collection(&ctx, &actor, json!({"source_type":"swanlake"})).await;
    let validation = collector
        .validate_saved(&actor, &source.id, source.version)
        .await
        .unwrap();
    assert_eq!(validation["executed"], false);
    assert_eq!(validation["schedule_registered"], false);
    assert!(query.calls.lock().unwrap().is_empty());
    let first = collect(&collector, &worker, &actor, &source, "metadata-first").await;
    assert_eq!(first["created_objects"], 2);
    assert_eq!(first["collected_columns"], 2);
    let initial = objects(&ctx, &actor).await;
    assert!(initial.iter().all(|object| object.version == 1));
    let second = collect(&collector, &worker, &actor, &source, "metadata-same").await;
    assert_eq!(second["unchanged_objects"], 2);
    assert_eq!(second["updated_objects"], 0);
    assert!(
        objects(&ctx, &actor)
            .await
            .iter()
            .all(|object| object.version == 1)
    );
    *query.rows.lock().unwrap() = vec![
        row("main", "orders", "id", 1),
        row("main", "orders", "amount", 2),
        row("main", "customers", "id", 1),
    ];
    let third = collect(&collector, &worker, &actor, &source, "metadata-changed").await;
    assert_eq!(third["updated_objects"], 1);
    assert_eq!(third["unchanged_objects"], 1);
    let observed = objects(&ctx, &actor).await;
    let orders = observed
        .iter()
        .find(|object| object.properties["table_name"] == "orders")
        .unwrap();
    assert_eq!(orders.version, 2);
    assert_eq!(orders.properties["columns"].as_array().unwrap().len(), 2);
    let previous = initial
        .iter()
        .find(|object| object.id == orders.id)
        .unwrap();
    assert_ne!(
        orders.properties["source_fingerprint"],
        previous.properties["source_fingerprint"]
    );
    assert!(
        ctx.metadata_catalog_repo
            .verified_binding(&actor.tenant_id, &orders.id, orders.version)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        ctx.metadata_catalog_repo
            .verified_binding(&actor.tenant_id, &orders.id, previous.version)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ctx.metadata_catalog_repo
            .verified_binding("foreign-tenant", &orders.id, orders.version)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        query
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|sql| sql.contains("information_schema.tables") && !sql.contains(" LIMIT "))
    );
}

#[actix_web::test]
async fn empty_complete_exact_scope_marks_missing_only_within_that_scope() {
    let (ctx, actor) = setup().await;
    let query = Arc::new(SnapshotQuery::default());
    *query.rows.lock().unwrap() = vec![
        row("main", "orders", "id", 1),
        row("analytics", "reports", "id", 1),
    ];
    let (collector, worker) = services(&ctx, &actor, query.clone());
    let full = collection(&ctx, &actor, json!({"source_type":"swanlake"})).await;
    collect(&collector, &worker, &actor, &full, "full-snapshot").await;
    query.rows.lock().unwrap().clear();
    let exact = collection(
        &ctx,
        &actor,
        json!({"source_type":"swanlake","schema_filter":"main"}),
    )
    .await;
    let result = collect(&collector, &worker, &actor, &exact, "main-missing").await;
    assert_eq!(result["collected_tables"], 0);
    assert_eq!(result["missing_objects"], 1);
    let objects = objects(&ctx, &actor).await;
    let missing = objects
        .iter()
        .find(|object| object.properties["schema_name"] == "main")
        .unwrap();
    let outside = objects
        .iter()
        .find(|object| object.properties["schema_name"] == "analytics")
        .unwrap();
    assert_eq!(missing.status, "inactive");
    assert_eq!(missing.properties["discovery_status"], "missing");
    assert_eq!(missing.version, 2);
    assert_eq!(outside.status, "published");
    assert_eq!(outside.version, 1);
    assert!(
        ctx.metadata_catalog_repo
            .verified_binding(&actor.tenant_id, &missing.id, missing.version)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ctx.metadata_catalog_repo
            .verified_binding(&actor.tenant_id, &outside.id, outside.version)
            .await
            .unwrap()
            .is_some()
    );
    let repeated = collect(&collector, &worker, &actor, &exact, "same-missing").await;
    assert_eq!(repeated["missing_objects"], 0);
    assert_eq!(
        ctx.product_resource_repo
            .get(&actor.tenant_id, "metadata_object", &missing.id)
            .await
            .unwrap()
            .unwrap()
            .version,
        2
    );
}

#[actix_web::test]
async fn real_query_failure_never_applies_a_partial_snapshot_or_marks_existing_objects_missing() {
    let (ctx, actor) = setup().await;
    let query = Arc::new(SnapshotQuery::default());
    *query.rows.lock().unwrap() = vec![row("main", "orders", "id", 1)];
    let (collector, worker) = services(&ctx, &actor, query.clone());
    let source = collection(&ctx, &actor, json!({"source_type":"swanlake"})).await;
    collect(
        &collector,
        &worker,
        &actor,
        &source,
        "before-engine-failure",
    )
    .await;
    query.fail.store(true, Ordering::SeqCst);
    let failed = collector
        .enqueue(
            &actor,
            &source.id,
            MetadataRunRequest {
                expected_version: source.version,
                idempotency_key: "actual-engine-failure".into(),
            },
        )
        .await
        .unwrap()
        .job;
    worker.run_once().await.unwrap();
    let actual = worker.get(&actor, &failed.id).await.unwrap();
    assert_eq!(actual.status, ProductJobStatus::Failed);
    assert!(actual.result.is_none());
    let objects = objects(&ctx, &actor).await;
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].version, 1);
    assert_eq!(objects[0].status, "published");
    let unsupported = collection(
        &ctx,
        &actor,
        json!({"source_type":"datasource","datasource_id":null}),
    )
    .await;
    assert!(matches!(
        collector
            .enqueue(
                &actor,
                &unsupported.id,
                MetadataRunRequest {
                    expected_version: unsupported.version,
                    idempotency_key: "unsupported-driver".into()
                }
            )
            .await,
        Err(AppError::BadRequest(_))
    ));
}
