mod common;
#[path = "model_publication_support/flight.rs"]
mod flight;

use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ResourceReference, ResourceWrite,
    repo::standard_governance::{StandardCoverage, StandardRevision},
};
use dts_server::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        product::{ProductService, ResourceAction},
        standards::{
            ColumnStandardRequest, ModelStandardsRequest, StandardGovernanceService,
            StandardVersionRequest, WithdrawStandardRequest,
        },
    },
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

struct Fixture {
    ctx: common::TestBackend,
    actor: AuthIdentity,
    engine: flight::TestFlight,
    product: ProductService,
    standards: StandardGovernanceService,
    model: ProductResource,
}

async fn resource(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    kind: &str,
    properties: Value,
    references: Vec<ResourceReference>,
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
                name: "真实模型发布协议测试".into(),
                description: None,
                parent_id: None,
                status: "draft".into(),
                properties,
                references,
            },
        })
        .await
        .unwrap()
}

async fn fixture() -> Fixture {
    let ctx = common::setup_test_backend().await.unwrap();
    let actor = ctx
        .auth
        .authenticate(&common::admin_token(&ctx).await)
        .await
        .unwrap();
    let engine = flight::TestFlight::start().await;
    let standards = StandardGovernanceService::new(
        ctx.standard_governance_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
    );
    let product = ProductService::new(
        ctx.product_resource_repo.clone(),
        engine.query.clone(),
        Some(actor.tenant_id.clone()),
    )
    .with_standards(Arc::new(standards.clone()));
    let layer = resource(
        &ctx,
        &actor,
        "warehouse_layer",
        json!({"prefix":"ods"}),
        vec![],
    )
    .await;
    let model = resource(
        &ctx, &actor, "data_model",
        json!({"layer_id":layer.id,"table_name":"main.orders","columns":[{"name":"id","data_type":"BIGINT","nullable":false}],"primary_key":[]}),
        vec![ResourceReference {field:"layer_id".into(),target_kind:"warehouse_layer".into(),target_id:layer.id.clone()}],
    ).await;
    Fixture {
        ctx,
        actor,
        engine,
        product,
        standards,
        model,
    }
}

impl Fixture {
    async fn publish(&self, model: &ProductResource) -> Result<Value, AppError> {
        self.product
            .action(
                &self.actor,
                "data_model",
                &model.id,
                "publish",
                ResourceAction {
                    expected_version: model.version,
                    payload: json!({}),
                },
            )
            .await
    }
    async fn current_model(&self) -> ProductResource {
        self.ctx
            .product_resource_repo
            .get(&self.actor.tenant_id, "data_model", &self.model.id)
            .await
            .unwrap()
            .unwrap()
    }
    async fn publications(&self) -> Vec<ProductResource> {
        self.ctx
            .product_resource_repo
            .list(&ProductResourceFilter {
                tenant_id: self.actor.tenant_id.clone(),
                kind: "model_publication".into(),
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
    async fn bind_field_standard(&self) -> (ProductResource, ProductResource, StandardRevision) {
        let source = resource(
            &self.ctx,
            &self.actor,
            "field_standard",
            json!({"data_type":"BIGINT","nullable":false,"definition":"已发布的非空标识字段"}),
            vec![],
        )
        .await;
        let published = self
            .standards
            .publish(&self.actor, "field_standard", &source.id, source.version)
            .await
            .unwrap();
        let revision = published.revision.unwrap();
        let bound = self
            .standards
            .bind_model(
                &self.actor,
                &self.model.id,
                ModelStandardsRequest {
                    expected_model_version: self.model.version,
                    expected_binding_version: None,
                    coverage: StandardCoverage::AllFields,
                    columns: vec![ColumnStandardRequest {
                        column_name: "id".into(),
                        field_standard: Some(StandardVersionRequest {
                            id: source.id,
                            version: revision.revision_version,
                        }),
                        naming_standard: None,
                    }],
                    table_naming: None,
                    upgrade: false,
                },
            )
            .await
            .unwrap();
        (bound.resource, published.resource, revision)
    }
    async fn withdraw(&self, source: &ProductResource, revision: &StandardRevision) {
        self.standards
            .withdraw(
                &self.actor,
                "field_standard",
                &source.id,
                WithdrawStandardRequest {
                    expected_version: source.version,
                    revision_version: revision.revision_version,
                    expected_state_version: revision.state_version,
                    reason: "真实发布版本撤回测试".into(),
                },
            )
            .await
            .unwrap();
    }
    fn create_calls(&self) -> usize {
        self.engine.observations.create_calls.load(Ordering::SeqCst)
    }
}

#[actix_web::test]
async fn first_publication_uses_flight_create_and_persists_verified_actual_receipt() {
    let f = fixture().await;
    let result = f.publish(&f.model).await.unwrap();
    assert_eq!(f.create_calls(), 1);
    assert_eq!(result["resource"]["status"], "published");
    assert_eq!(result["resource"]["version"], f.model.version + 2);
    assert_eq!(result["result"]["reconciled"], false);
    assert_eq!(result["result"]["rows_affected"], 0);
    assert_eq!(
        result["result"]["physical_binding"]["table_catalog"],
        "lake"
    );
    assert_eq!(
        result["result"]["physical_columns"][0]["data_type"],
        "BIGINT"
    );
    let saved = f.publications().await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].status, "published");
    assert_eq!(saved[0].properties["receipt"], result["result"]);
    assert_eq!(saved[0].properties["standards"]["state"], "not_configured");
    assert_eq!(saved[0].properties["model_version"], f.model.version + 1);
}

#[actix_web::test]
async fn verification_failure_records_unknown_then_reconciles_without_second_create() {
    let f = fixture().await;
    f.engine
        .observations
        .fail_next_columns
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        f.publish(&f.model).await,
        Err(AppError::Conflict(_))
    ));
    let unknown = f.current_model().await;
    assert_eq!(unknown.status, "publish_unknown");
    assert_eq!(f.create_calls(), 1);
    let first = f.publications().await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].status, "unknown");
    assert_eq!(first[0].properties["standards"]["state"], "invalidated");
    let result = f.publish(&unknown).await.unwrap();
    assert_eq!(result["resource"]["status"], "published");
    assert_eq!(result["result"]["reconciled"], true);
    assert!(result["result"].get("rows_affected").is_none());
    assert_eq!(
        f.create_calls(),
        1,
        "unknown recovery must not repeat physical CREATE"
    );
    let saved = f.publications().await;
    assert_eq!(saved.len(), 2);
    assert_eq!(saved.iter().filter(|p| p.status == "unknown").count(), 1);
    assert_eq!(saved.iter().filter(|p| p.status == "published").count(), 1);
}

#[actix_web::test]
async fn recovery_when_actual_table_is_missing_remains_unknown_and_never_creates() {
    let f = fixture().await;
    f.engine
        .observations
        .fail_next_columns
        .store(true, Ordering::SeqCst);
    assert!(f.publish(&f.model).await.is_err());
    let unknown = f.current_model().await;
    f.engine.observations.created.store(false, Ordering::SeqCst);
    assert!(matches!(
        f.publish(&unknown).await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(f.current_model().await.status, "publish_unknown");
    assert_eq!(
        f.create_calls(),
        1,
        "missing table is not permission to replay uncertain CREATE"
    );
    assert!(f.publications().await.iter().all(|p| p.status == "unknown"));
}

#[actix_web::test]
async fn physical_type_difference_cannot_be_reported_as_published_or_overwritten() {
    let f = fixture().await;
    f.engine
        .observations
        .wrong_type
        .store(true, Ordering::SeqCst);
    assert!(matches!(
        f.publish(&f.model).await,
        Err(AppError::Conflict(_))
    ));
    let unknown = f.current_model().await;
    assert_eq!(unknown.status, "publish_unknown");
    assert!(matches!(
        f.publish(&unknown).await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(f.create_calls(), 1);
    assert!(f.publications().await.iter().all(|p| p.status == "unknown"));
}

#[actix_web::test]
async fn withdrawn_exact_bound_standard_blocks_gate_before_any_engine_request() {
    let f = fixture().await;
    let (model, source, revision) = f.bind_field_standard().await;
    f.withdraw(&source, &revision).await;
    assert!(matches!(
        f.publish(&model).await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(f.create_calls(), 0);
    assert!(f.engine.observations.sql.lock().unwrap().is_empty());
    assert_eq!(f.current_model().await.status, "draft");
    assert!(f.publications().await.is_empty());
}

#[actix_web::test]
async fn withdrawal_after_create_keeps_real_effect_and_records_invalidated_unknown() {
    let f = fixture().await;
    let (model, source, revision) = f.bind_field_standard().await;
    f.engine
        .observations
        .pause_columns
        .store(true, Ordering::SeqCst);
    let product = f.product.clone();
    let actor = f.actor.clone();
    let model_id = model.id.clone();
    let action = tokio::spawn(async move {
        product
            .action(
                &actor,
                "data_model",
                &model_id,
                "publish",
                ResourceAction {
                    expected_version: model.version,
                    payload: json!({}),
                },
            )
            .await
    });
    tokio::time::timeout(
        Duration::from_secs(5),
        f.engine.observations.columns_started.notified(),
    )
    .await
    .unwrap();
    assert_eq!(f.create_calls(), 1);
    f.withdraw(&source, &revision).await;
    f.engine.observations.release_columns.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(5), action)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(AppError::Conflict(_))));
    assert!(f.engine.observations.created.load(Ordering::SeqCst));
    assert_eq!(f.current_model().await.status, "publish_unknown");
    let saved = f.publications().await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].status, "unknown");
    assert_eq!(saved[0].properties["standards"]["state"], "invalidated");
    assert_eq!(
        saved[0].properties["receipt"]["physical_binding"]["table_catalog"],
        "lake"
    );
    assert_eq!(
        saved[0].properties["receipt"]["physical_columns"][0]["data_type"],
        "BIGINT"
    );
}
