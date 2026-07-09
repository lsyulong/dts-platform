//! Tenant API integration tests

mod common;

use actix_web::{test, web, App};
use dts_server::api::tenants;
use dts_server::services::tenant_svc::TenantService;
use pretty_assertions::assert_eq;
use serde_json::json;

#[actix_web::test]
async fn create_tenant_successfully() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/tenants")
                        .route("", web::post().to(tenants::create))
                        .route("", web::get().to(tenants::list)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "测试租户",
            "slug": "test-tenant"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "测试租户");
    assert_eq!(body["slug"], "test-tenant");
    assert!(body["admin_username"].is_string());
    assert!(body["message"].is_string());
}

#[actix_web::test]
async fn create_tenant_duplicate_slug_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/tenants")
                        .route("", web::post().to(tenants::create))
                        .route("", web::get().to(tenants::list)),
                ),
            ),
    )
    .await;

    // First creation
    let req1 = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "First",
            "slug": "duplicate-slug"
        }))
        .to_request();
    let resp1 = test::call_service(&app, req1).await;
    assert_eq!(resp1.status(), 201);

    // Duplicate slug
    let req2 = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "Second",
            "slug": "duplicate-slug"
        }))
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert!(resp2.status().is_client_error(), "Expected client error for duplicate slug");
}

#[actix_web::test]
async fn list_tenants_returns_all_tenants() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tenants").route("", web::get().to(tenants::list))),
            ),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/tenants").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    let data = body["data"].as_array().unwrap();
    assert!(!data.is_empty(), "Tenant list should not be empty");
    assert!(
        data.iter().any(|t| t["slug"] == "default"),
        "Seed tenant 'default' should be in the list"
    );
}

#[actix_web::test]
async fn create_tenant_missing_name_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/tenants").route("", web::post().to(tenants::create)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "slug": "only-slug"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Missing name should return 400");
}

#[actix_web::test]
async fn create_tenant_missing_slug_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/tenants").route("", web::post().to(tenants::create)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "Only Name"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Missing slug should return 400");
}

#[actix_web::test]
async fn create_tenant_empty_body_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/tenants").route("", web::post().to(tenants::create)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Empty body should return 400");
}
