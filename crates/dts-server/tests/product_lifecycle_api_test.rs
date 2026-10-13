mod common;

use actix_web::{App, test, web};
use dts_backend::types::{CreateRoleParams, CreateUserParams};
use dts_server::{
    api::{development_revisions, product, standards},
    config::SwanLakeConfig,
    services::{
        development_revisions::DevelopmentRevisionsService,
        product::{ProductService, ResourceInput},
        query_svc::QueryService,
        runtime_configuration::RuntimeConfigurationService,
        standards::StandardGovernanceService,
    },
};
use serde_json::{Value, json};

fn routes(config: &mut web::ServiceConfig) {
    config
        .route(
            "/resources/{kind}/{id}/actions/{action}",
            web::post().to(product::action),
        )
        .route(
            "/standards/{kind}/{id}/revisions",
            web::get().to(standards::revisions),
        )
        .route(
            "/development/{kind}/{id}/revisions",
            web::get().to(development_revisions::history),
        );
}

#[actix_web::test]
async fn runtime_policy_http_requires_actual_reason_and_permissions_and_replays_without_restoring_state()
 {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let resources = resources(&ctx);
    let config=resources.create(&actor,"development_config",ResourceInput{code:"runtime_policy".into(),name:"Runtime policy".into(),description:None,parent_id:None,properties:json!({"scope":"shared","variables":{},"timeout_seconds":1,"concurrency":1})}).await.unwrap();
    ctx.user_repo
        .create(&CreateUserParams {
            id: "config_reader".into(),
            tenant_id: actor.tenant_id.clone(),
            username: "config_reader".into(),
            email: "configreader@test".into(),
            password_hash: "unused".into(),
            display_name: None,
        })
        .await
        .unwrap();
    ctx.role_repo
        .create_with_permissions(&CreateRoleParams {
            id: "config_read_role".into(),
            tenant_id: actor.tenant_id.clone(),
            name: "config_reader".into(),
            description: "Pure validation only".into(),
            is_system: false,
            permissions: vec!["development:read".into()],
        })
        .await
        .unwrap();
    ctx.user_repo
        .assign_role("config_reader", "config_read_role")
        .await
        .unwrap();
    let reader = common::test_token(&ctx, "config_reader").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .app_data(web::Data::new(resources.clone()))
            .app_data(web::Data::new(RuntimeConfigurationService::new(
                ctx.runtime_configuration_repo.clone(),
                ctx.product_resource_repo.clone(),
            )))
            .configure(routes),
    )
    .await;
    let path = format!("/resources/development_config/{}/actions", config.id);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/validate"))
            .insert_header(("Authorization", format!("Bearer {reader}")))
            .set_json(json!({"expected_version":1,"payload":{}}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["result"]["valid"], true);
    assert_eq!(body["result"]["executed"], false);
    let key = uuid::Uuid::new_v4().to_string();
    let payload = json!({"reason":"Reviewed resource budget","idempotency_key":key});
    let denied = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/publish"))
            .insert_header(("Authorization", format!("Bearer {reader}")))
            .set_json(json!({"expected_version":1,"payload":payload}))
            .to_request(),
    )
    .await;
    assert_eq!(denied.status(), 403);
    for (action, invalid) in [
        ("publish", json!({"reason":"","idempotency_key":key})),
        (
            "publish",
            json!({"reason":"Reviewed","idempotency_key":key,"executed":true}),
        ),
        ("validate", json!({"valid":true})),
        (
            "publish",
            json!({"reason":"Reviewed","idempotency_key":key,"expected_version":99}),
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("{path}/{action}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"expected_version":1,"payload":invalid}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 400);
    }
    assert_eq!(
        resources
            .get(&actor, &config.kind, &config.id)
            .await
            .unwrap()
            .version,
        1
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/publish"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"expected_version":1,"payload":payload}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["resource"]["version"], 2);
    assert_eq!(body["executed"], false);
    let response=test::call_service(&app,test::TestRequest::post().uri(&format!("{path}/withdraw")).insert_header(("Authorization",format!("Bearer {token}"))).set_json(json!({"expected_version":2,"payload":{"reason":"Retire policy","idempotency_key":uuid::Uuid::new_v4().to_string()}})).to_request()).await;
    assert_eq!(response.status(), 200);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/publish"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"expected_version":1,"payload":payload}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["resource"]["status"], "inactive");
    assert_eq!(body["resource"]["version"], 3);
    assert_eq!(body["result"]["replayed"], true);
    assert_eq!(body["result"]["resource"]["status"], "published");
}

fn resources(ctx: &common::TestBackend) -> ProductService {
    ProductService::new(
        ctx.product_resource_repo.clone(),
        QueryService::new(&SwanLakeConfig::default()),
        None,
    )
}

#[actix_web::test]
async fn periodic_resource_save_requires_a_real_schedule_not_just_text_and_number_types() {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let service = resources(&ctx);
    for (index, schedule) in [
        json!({"cron_expression":"0 25 * * *","timezone":"UTC"}),
        json!({"cron_expression":"0 8 * * *","timezone":"Asia/DoesNotExist"}),
        json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":-1}),
        json!({"cron_expression":"0 8 * * *","timezone":"UTC","retry_limit":11}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut properties = schedule;
        properties["definition"] =
            json!({"nodes":[{"id":"query","name":"Query","sql":"SELECT 1","depends_on":[]}]});
        assert!(
            service
                .create(
                    &actor,
                    "development_workflow",
                    ResourceInput {
                        code: format!("invalid_schedule_{index}"),
                        name: "Invalid schedule".into(),
                        description: None,
                        parent_id: None,
                        properties,
                    }
                )
                .await
                .is_err()
        );
    }
    let valid = service.create(&actor, "development_workflow", ResourceInput {
        code:"valid_periodic_definition".into(),name:"Periodic definition".into(),description:None,parent_id:None,
        properties:json!({"definition":{"nodes":[{"id":"query","name":"Query","sql":"SELECT 1","depends_on":[]}]},"cron_expression":"0 8 * * MON-FRI","timezone":"Asia/Shanghai","retry_limit":3}),
    }).await.unwrap();
    assert_eq!(valid.status, "draft");
    assert_eq!(valid.version, 1);
    assert_eq!(valid.properties["timezone"], "Asia/Shanghai");
    assert_eq!(valid.properties["retry_limit"], 3);
    assert!(valid.properties.get("schedule_registered").is_none());
}

#[actix_web::test]
async fn actual_naming_sample_and_read_execute_authority_are_not_replaced_by_definition_success() {
    let ctx = common::setup_test_backend().await.unwrap();
    let admin = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&admin).await.unwrap();
    let resources = resources(&ctx);
    let definition = resources
        .create(
            &actor,
            "naming_standard",
            ResourceInput {
                code: "table_names".into(),
                name: "Table naming".into(),
                description: None,
                parent_id: None,
                properties: json!({"object_type":"table","pattern":"ods_[a-z]+"}),
            },
        )
        .await
        .unwrap();
    ctx.user_repo
        .create(&CreateUserParams {
            id: "standard_reader".into(),
            tenant_id: actor.tenant_id.clone(),
            username: "standard_reader".into(),
            email: "reader@example.test".into(),
            password_hash: "unused".into(),
            display_name: None,
        })
        .await
        .unwrap();
    ctx.role_repo
        .create_with_permissions(&CreateRoleParams {
            id: "standard_validator".into(),
            tenant_id: actor.tenant_id.clone(),
            name: "standard_validator".into(),
            description: "May validate, may not publish".into(),
            is_system: false,
            permissions: vec!["standards:read".into(), "standards:execute".into()],
        })
        .await
        .unwrap();
    ctx.user_repo
        .assign_role("standard_reader", "standard_validator")
        .await
        .unwrap();
    let token = common::test_token(&ctx, "standard_reader").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .app_data(web::Data::new(resources.clone()))
            .app_data(web::Data::new(StandardGovernanceService::new(
                ctx.standard_governance_repo.clone(),
                ctx.product_resource_repo.clone(),
                ctx.auth.clone(),
            )))
            .configure(routes),
    )
    .await;
    let path = format!("/resources/naming_standard/{}/actions", definition.id);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/validate"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"expected_version":1,"payload":{"sample":"wrong_name"}}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["result"]["definition_valid"], true);
    assert_eq!(body["result"]["valid"], false);
    assert_eq!(body["result"]["name_validation"]["matched"], false);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/publish"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"expected_version":1,"payload":{}}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{path}/validate"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"expected_version":1,"payload":{"sample":"ods_orders","version":8}}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);
    assert_eq!(
        resources
            .get(&actor, "naming_standard", &definition.id)
            .await
            .unwrap()
            .version,
        1
    );
}

#[actix_web::test]
async fn saving_a_real_development_revision_is_immutable_and_does_not_publish_or_enqueue() {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let actor = ctx.auth.authenticate(&token).await.unwrap();
    let resources = resources(&ctx);
    let source = resources
        .create(
            &actor,
            "sql_document",
            ResourceInput {
                code: "saved_query".into(),
                name: "Saved query".into(),
                description: None,
                parent_id: None,
                properties: json!({"sql":"SELECT 1","parameters":[]}),
            },
        )
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .app_data(web::Data::new(resources.clone()))
            .app_data(web::Data::new(DevelopmentRevisionsService::new(
                ctx.development_revision_repo.clone(),
            )))
            .configure(routes),
    )
    .await;
    let path = format!("/resources/sql_document/{}/actions/save_version", source.id);
    let payload = json!({"expected_version":1,"payload":{"reason":"Review first draft","idempotency_key":uuid::Uuid::new_v4().to_string()}});
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(&payload)
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let first: Value = test::read_body_json(response).await;
    assert_eq!(first["resource"]["status"], "draft");
    assert_eq!(first["resource"]["version"], 1);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(&payload)
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let repeated: Value = test::read_body_json(response).await;
    assert_eq!(repeated["result"]["revision"], first["result"]["revision"]);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/development/sql_document/{}/revisions",
                source.id
            ))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let history: Value = test::read_body_json(response).await;
    assert_eq!(history["total"], 1);
    let jobs = ctx
        .product_job_repo
        .list(&dts_backend::repo::product_job::ProductJobFilter {
            tenant_id: actor.tenant_id.clone(),
            actor_id: None,
            source_kind: Some("sql_document".into()),
            source_id: Some(source.id),
            status: None,
            limit: 20,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(jobs.total, 0);
}
