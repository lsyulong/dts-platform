//! 租户 API 集成测试

mod common;

use actix_web::{test, web, App};
use dts_server::api::tenants;
use dts_server::services::auth_svc::AuthService;
use dts_server::services::tenant_svc::TenantService;
use serde_json::json;

#[actix_web::test]
async fn create_tenant_successfully() {
    let pool = common::setup_test_db().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());
    let tenant_svc = TenantService::new(pool.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(
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
}

#[actix_web::test]
async fn create_tenant_duplicate_slug_fails() {
    let pool = common::setup_test_db().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());
    let tenant_svc = TenantService::new(pool.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/tenants")
                            .route("", web::post().to(tenants::create))
                            .route("", web::get().to(tenants::list)),
                    ),
            ),
    )
    .await;

    // 第一次创建
    let req1 = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "First",
            "slug": "duplicate-slug"
        }))
        .to_request();
    let resp1 = test::call_service(&app, req1).await;
    assert_eq!(resp1.status(), 201);

    // 重复 slug
    let req2 = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .set_json(&json!({
            "name": "Second",
            "slug": "duplicate-slug"
        }))
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert!(resp2.status().is_client_error());
}

#[actix_web::test]
async fn list_tenants_returns_all_tenants() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());
    let tenant_svc = TenantService::new(pool.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/tenants")
                            .route("", web::get().to(tenants::list)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/tenants")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    assert!(!body["data"].as_array().unwrap().is_empty());
}
