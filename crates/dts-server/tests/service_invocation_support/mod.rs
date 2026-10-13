mod flight;
use super::{common, masking};
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ResourceWrite,
    repo::product_job::ProductJobStatus,
};
use dts_server::{
    api::query::QueryAccess,
    models::auth::AuthIdentity,
    services::{
        data_access::QueryAuthorizationService,
        data_masking::MaskingService,
        data_services::DataApiService,
        metadata::{MetadataRunRequest, MetadataService},
        product::{ResourceAction, ResourceInput},
        product_jobs::ProductJobService,
        service_access::{CredentialKey, ServiceAccessService},
        service_invocation::ServiceInvocationService,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

pub use flight::Observations;
pub struct Fixture {
    pub ctx: common::TestBackend,
    pub actor: AuthIdentity,
    pub invocation: ServiceInvocationService,
    pub access: ServiceAccessService,
    pub masking: Arc<MaskingService>,
    pub api: ProductResource,
    pub application: ProductResource,
    pub credential_id: String,
    pub secret: String,
    pub observations: Arc<Observations>,
    pub rule: ProductResource,
    _flight: flight::TestFlight,
}
pub async fn resource(
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
                name: "外部调用模块测试".into(),
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
fn input(code: &str, properties: Value) -> ResourceInput {
    ResourceInput {
        code: code.into(),
        name: code.into(),
        description: None,
        parent_id: None,
        properties,
    }
}
fn action(version: i64, key: &str) -> ResourceAction {
    ResourceAction {
        expected_version: version,
        payload: json!({"idempotency_key":key,"reason":"Reviewed"}),
    }
}
async fn actual_approver(ctx: &common::TestBackend, actor: &AuthIdentity) -> AuthIdentity {
    let id = uuid::Uuid::new_v4().to_string();
    ctx.user_repo
        .create(&dts_backend::types::CreateUserParams {
            id: id.clone(),
            tenant_id: actor.tenant_id.clone(),
            username: "external-approver".into(),
            email: "approver@module.test".into(),
            password_hash: "test-only-password-hash".into(),
            display_name: None,
        })
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
    ctx.user_repo.assign_role(&id, &role.id).await.unwrap();
    let token = common::test_token(ctx, &id).await;
    ctx.auth.authenticate(&token).await.unwrap()
}
async fn collected_metadata(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    flight: &flight::TestFlight,
    access: &QueryAccess,
    guard: Arc<QueryAuthorizationService>,
    masking: Arc<MaskingService>,
) -> ProductResource {
    let worker = ProductJobService::new(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.auth.clone(),
        flight.query.clone(),
        access.clone(),
    )
    .with_metadata_repo(ctx.metadata_catalog_repo.clone())
    .with_query_authorization(guard)
    .with_masking(masking);
    let collector = MetadataService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.auth.clone(),
        access.clone(),
    );
    let definition = resource(
        ctx,
        actor,
        "metadata_collection",
        json!({"source_type":"swanlake"}),
    )
    .await;
    let run = collector
        .enqueue(
            actor,
            &definition.id,
            MetadataRunRequest {
                expected_version: definition.version,
                idempotency_key: "actual-flight-collection".into(),
            },
        )
        .await
        .unwrap();
    assert!(worker.run_once().await.unwrap());
    assert_eq!(
        worker.get(actor, &run.job.id).await.unwrap().status,
        ProductJobStatus::Succeeded
    );
    let mut page = ctx
        .product_resource_repo
        .list(&ProductResourceFilter {
            tenant_id: actor.tenant_id.clone(),
            kind: "metadata_object".into(),
            q: None,
            status: Some("published".into()),
            created_by: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    page.data.remove(0)
}
async fn actual_mask(
    ctx: &common::TestBackend,
    actor: &AuthIdentity,
    metadata: &ProductResource,
    masking: &MaskingService,
) -> ProductResource {
    let rule = resource(
        ctx,
        actor,
        "masking_rule",
        json!({
            "object_id":metadata.id,"column_name":"amount","method":"hash","parameters":{}
        }),
    )
    .await;
    let rule = masking
        .publish(actor, "masking_rule", &rule.id, rule.version)
        .await
        .unwrap();
    let group = resource(
        ctx,
        actor,
        "user_group",
        json!({"member_ids":[actor.sub],"masking_rule_id":rule.id}),
    )
    .await;
    masking
        .publish(actor, "user_group", &group.id, group.version)
        .await
        .unwrap();
    rule
}
pub async fn fixture(rate: u32) -> Fixture {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let flight = flight::TestFlight::start().await;
    let query_access = QueryAccess {
        tenant_id: Some(actor.tenant_id.clone()),
    };
    let guard = Arc::new(
        QueryAuthorizationService::new(
            ctx.data_access_repo.clone(),
            ctx.metadata_catalog_repo.clone(),
            query_access.clone(),
        )
        .with_job_repo(ctx.product_job_repo.clone()),
    );
    let masking = masking::service(&ctx);
    let metadata = collected_metadata(
        &ctx,
        &actor,
        &flight,
        &query_access,
        guard.clone(),
        masking.clone(),
    )
    .await;
    let rule = actual_mask(&ctx, &actor, &metadata, &masking).await;
    let api_service = DataApiService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.auth.clone(),
        query_access,
    );
    let api = resource(
        &ctx,
        &actor,
        "data_api",
        json!({
            "metadata_object_id":metadata.id,"sql":"SELECT region, amount FROM lake.main.orders",
            "parameters":[],"response_schema":[],"row_limit":2,"rate_limit":rate
        }),
    )
    .await;
    let api = api_service
        .publish(&actor, &api.id, api.version)
        .await
        .unwrap();
    let api: ProductResource = serde_json::from_value(api["resource"].clone()).unwrap();
    let key_dir = tempfile::tempdir().unwrap();
    let key = CredentialKey::load_or_create(key_dir.path().to_str().unwrap()).unwrap();
    let access = ServiceAccessService::new(
        ctx.service_access_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.user_repo.clone(),
        key,
    );
    let application = access
        .save_application(&actor, input("flight-application", json!({})))
        .await
        .unwrap();
    let issued = access
        .issue_credential(
            &actor,
            &application.id,
            ResourceAction {
                expected_version: application.version,
                payload: json!({"idempotency_key":"actual-credential"}),
            },
        )
        .await
        .unwrap();
    let subscription=access.save_subscription(&actor,input("flight-subscription",json!({
        "application_id":application.id,"api_id":api.id,"purpose":"Controlled protocol module test"
    }))).await.unwrap();
    let pending = access
        .subscription_action(
            &actor,
            &subscription.id,
            "submit",
            action(subscription.version, "submit"),
        )
        .await
        .unwrap();
    let approver = actual_approver(&ctx, &actor).await;
    let approved = access
        .subscription_action(
            &approver,
            &subscription.id,
            "approve",
            action(pending.subscription.version, "approve"),
        )
        .await
        .unwrap();
    assert_eq!(approved.subscription.status, "approved");
    let invocation = ServiceInvocationService::new(
        access.clone(),
        api_service,
        Arc::new(flight.query.clone()),
        guard,
        masking.clone(),
    );
    Fixture {
        observations: flight.observations.clone(),
        ctx,
        actor,
        invocation,
        access,
        masking,
        api,
        application,
        credential_id: issued.credential.id,
        secret: issued.secret.unwrap(),
        rule,
        _flight: flight,
    }
}
pub async fn query_audits(f: &Fixture) -> Vec<dts_backend::repo::data_access::DataAccessAudit> {
    f.ctx
        .data_access_repo
        .list_audit(&dts_backend::repo::data_access::DataAccessAuditFilter {
            tenant_id: f.actor.tenant_id.clone(),
            actor_id: f.actor.sub.clone(),
            limit: 100,
            offset: 0,
        })
        .await
        .unwrap()
        .data
        .into_iter()
        .filter(|event| event.action == "query")
        .collect()
}
