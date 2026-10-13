mod common;
#[path = "product_job_service_support/masking.rs"]
mod masking;
use dts_backend::{
    CreateResourceParams, ProductResource, ResourceWrite,
    repo::product_job::{ProductJobLookup, ProductJobStatus},
};
use dts_server::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::QueryAuthorizationService,
        product_exports::{ExportFormat, ExportRequest, ResultExportService},
        product_jobs::{
            ProductJobService, ReadOnlyQueryExecutor, ResultAccess, RetryRequest, RunRequest,
        },
        query_svc::{ColumnMeta, QueryResult},
        runtime_configuration::{ConfigurationActionRequest, RuntimeConfigurationService},
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
struct Query {
    calls: Mutex<Vec<String>>,
    block: AtomicBool,
}
#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for Query {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        self.calls.lock().unwrap().push(sql.into());
        if self.block.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(QueryResult {
            columns: vec![ColumnMeta {
                name: "value".into(),
                data_type: "Int64".into(),
                is_numeric: true,
            }],
            rows: vec![json!({"value":1})],
            row_count: 1,
            suggested_chart: None,
        })
    }
}
async fn source(
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
            actor_id: actor.sub.clone(),
            kind: kind.into(),
            fields: ResourceWrite {
                code: id,
                name: "Actual runtime configuration fixture".into(),
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
async fn setup() -> (
    common::TestBackend,
    AuthIdentity,
    Arc<Query>,
    ProductJobService,
    RuntimeConfigurationService,
) {
    let ctx = common::setup_test_backend().await.unwrap();
    let actor = ctx
        .auth
        .authenticate(&common::admin_token(&ctx).await)
        .await
        .unwrap();
    let query = Arc::new(Query::default());
    let access = QueryAccess {
        tenant_id: Some(actor.tenant_id.clone()),
    };
    let jobs = ProductJobService::with_executor(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        query.clone(),
        access.clone(),
    )
    .with_query_authorization(Arc::new(QueryAuthorizationService::new(
        ctx.data_access_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        access,
    )))
    .with_masking(masking::service(&ctx))
    .with_runtime_configuration(ctx.runtime_configuration_repo.clone());
    let configs = RuntimeConfigurationService::new(
        ctx.runtime_configuration_repo.clone(),
        ctx.product_resource_repo.clone(),
    );
    (ctx, actor, query, jobs, configs)
}
fn command(version: i64) -> ConfigurationActionRequest {
    ConfigurationActionRequest {
        expected_version: version,
        reason: "Reviewed runtime policy".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    }
}
fn run(version: i64, parameters: Value) -> RunRequest {
    RunRequest {
        expected_version: version,
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        parameters,
    }
}
async fn configured(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    jobs: &ProductJobService,
    configs: &RuntimeConfigurationService,
) -> (ProductResource, ProductResource) {
    let config=source(ctx,actor,"development_config",json!({"scope":"shared","variables":{"id":"9007199254740993"},"timeout_seconds":1,"concurrency":1})).await;
    let config = configs
        .publish(actor, &config.id, command(1))
        .await
        .unwrap()
        .resource;
    let src=source(ctx,actor,"sql_document",json!({"sql":"SELECT $id AS value","parameters":[{"name":"id","data_type":"BIGINT","required":true,"default_value":"1"}],"configuration_id":config.id})).await;
    let result = jobs
        .publish(actor, &src.kind, &src.id, src.version)
        .await
        .unwrap();
    assert_eq!(result["executed"], false);
    (
        config,
        serde_json::from_value(result["resource"].clone()).unwrap(),
    )
}
#[actix_web::test]
async fn typed_configuration_and_run_overrides_preserve_exact_values_and_real_saved_results() {
    let (ctx, actor, query, jobs, configs) = setup().await;
    let (config, src) = configured(&ctx, &actor, &jobs, &configs).await;
    assert!(query.calls.lock().unwrap().is_empty());
    let first = jobs
        .enqueue(&actor, &src.kind, &src.id, run(src.version, json!({})))
        .await
        .unwrap()
        .job;
    assert_eq!(first.parameters["id"], "9007199254740993");
    assert_eq!(
        first.definition_snapshot["runtime_configuration"]["configuration_version"],
        config.version
    );
    assert!(jobs.run_once().await.unwrap());
    assert_eq!(
        jobs.get(&actor, &first.id).await.unwrap().result_access,
        ResultAccess::Authorized
    );
    assert!(query.calls.lock().unwrap()[0].contains("9007199254740993"));
    let second = jobs
        .enqueue(
            &actor,
            &src.kind,
            &src.id,
            run(src.version, json!({"id":"9223372036854775807"})),
        )
        .await
        .unwrap()
        .job;
    assert_eq!(second.parameters["id"], "9223372036854775807");
    jobs.run_once().await.unwrap();
    assert!(
        jobs.enqueue(
            &actor,
            &src.kind,
            &src.id,
            run(src.version, json!({"undeclared":1}))
        )
        .await
        .is_err()
    );
    assert!(
        jobs.enqueue(
            &actor,
            &src.kind,
            &src.id,
            run(src.version, json!({"id":"9223372036854775808"}))
        )
        .await
        .is_err()
    );
    configs
        .withdraw(&actor, &config.id, command(config.version))
        .await
        .unwrap();
    let restricted = jobs.get(&actor, &first.id).await.unwrap();
    assert_eq!(restricted.status, ProductJobStatus::Succeeded);
    assert_eq!(restricted.result_access, ResultAccess::Restricted);
    assert!(restricted.nodes[0].result.is_none());
    let durable = ctx
        .product_job_repo
        .get(&ProductJobLookup {
            tenant_id: actor.tenant_id.clone(),
            job_id: first.id.clone(),
            actor_id: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert!(durable.nodes[0].result.is_some());
    let exporter = ResultExportService::new(Arc::new(jobs.clone()));
    assert!(
        exporter
            .export(
                &actor,
                &first.id,
                ExportRequest {
                    node_id: first.nodes[0].id.clone(),
                    expected_source_version: src.version,
                    format: ExportFormat::Csv
                }
            )
            .await
            .is_err()
    );
    assert!(
        jobs.retry(
            &actor,
            &first.id,
            RetryRequest {
                idempotency_key: uuid::Uuid::new_v4().to_string()
            }
        )
        .await
        .is_err()
    );
}
#[actix_web::test]
async fn withdrawal_before_worker_execution_causes_zero_query_calls() {
    let (ctx, actor, query, jobs, configs) = setup().await;
    let (config, src) = configured(&ctx, &actor, &jobs, &configs).await;
    let job = jobs
        .enqueue(&actor, &src.kind, &src.id, run(src.version, json!({})))
        .await
        .unwrap()
        .job;
    configs
        .withdraw(&actor, &config.id, command(config.version))
        .await
        .unwrap();
    jobs.run_once().await.unwrap();
    assert!(query.calls.lock().unwrap().is_empty());
    assert_eq!(
        jobs.get(&actor, &job.id).await.unwrap().status,
        ProductJobStatus::Failed
    );
    assert!(
        jobs.retry(
            &actor,
            &job.id,
            RetryRequest {
                idempotency_key: uuid::Uuid::new_v4().to_string()
            }
        )
        .await
        .is_err()
    );
}
#[actix_web::test]
async fn configured_timeout_finishes_failed_and_releases_the_real_execution_slot() {
    let (ctx, actor, query, jobs, configs) = setup().await;
    let (_, src) = configured(&ctx, &actor, &jobs, &configs).await;
    query.block.store(true, Ordering::SeqCst);
    let job = jobs
        .enqueue(&actor, &src.kind, &src.id, run(src.version, json!({})))
        .await
        .unwrap()
        .job;
    let before = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(5), jobs.run_once())
        .await
        .unwrap()
        .unwrap();
    assert!(before.elapsed() >= std::time::Duration::from_millis(900));
    assert_eq!(
        jobs.get(&actor, &job.id).await.unwrap().status,
        ProductJobStatus::Failed
    );
    query.block.store(false, Ordering::SeqCst);
    let next = jobs
        .enqueue(&actor, &src.kind, &src.id, run(src.version, json!({})))
        .await
        .unwrap()
        .job;
    jobs.run_once().await.unwrap();
    assert_eq!(
        jobs.get(&actor, &next.id).await.unwrap().status,
        ProductJobStatus::Succeeded
    );
}
#[actix_web::test]
async fn source_policy_checks_actual_parameter_names_types_and_scope_without_query_execution() {
    let (ctx, actor, query, jobs, configs) = setup().await;
    let src=source(&ctx,&actor,"sql_document",json!({"sql":"SELECT $id AS value","parameters":[{"name":"id","data_type":"BIGINT","required":true}]})).await;
    let bad=source(&ctx,&actor,"development_config",json!({"scope":"source","source_kind":"sql_document","source_id":src.id,"variables":{"id":"bad"}})).await;
    assert!(configs.validate(&actor, &bad.id, 1).await.is_err());
    assert!(configs.publish(&actor, &bad.id, command(1)).await.is_err());
    assert_eq!(
        ctx.product_resource_repo
            .get(&actor.tenant_id, &bad.kind, &bad.id)
            .await
            .unwrap()
            .unwrap()
            .version,
        1
    );
    let bad_name=source(&ctx,&actor,"development_config",json!({"scope":"source","source_kind":"sql_document","source_id":src.id,"variables":{"other":1}})).await;
    assert!(
        configs
            .publish(&actor, &bad_name.id, command(1))
            .await
            .is_err()
    );
    let valid=source(&ctx,&actor,"development_config",json!({"scope":"source","source_kind":"sql_document","source_id":src.id,"variables":{"id":"42"}})).await;
    assert_eq!(
        configs.validate(&actor, &valid.id, 1).await.unwrap()["valid"],
        true
    );
    let valid = configs
        .publish(&actor, &valid.id, command(1))
        .await
        .unwrap()
        .resource;
    let other = source(
        &ctx,
        &actor,
        "sql_document",
        json!({"sql":"SELECT 1","configuration_id":valid.id}),
    )
    .await;
    assert!(
        jobs.publish(&actor, &other.kind, &other.id, 1)
            .await
            .is_err()
    );
    assert!(query.calls.lock().unwrap().is_empty());
}
