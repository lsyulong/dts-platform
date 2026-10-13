// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tenant API integration tests

mod common;

use actix_web::{App, test, web};
use dts_server::api::tenants;
use dts_server::services::tenant_svc::TenantService;
use pretty_assertions::assert_eq;
use serde_json::json;

#[actix_web::test]
async fn create_tenant_successfully() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
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
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "name": "测试租户",
            "slug": "test-tenant", "admin_password": "initial-strong-password"
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
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
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
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "name": "First",
            "slug": "duplicate-slug", "admin_password": "initial-strong-password"
        }))
        .to_request();
    let resp1 = test::call_service(&app, req1).await;
    assert_eq!(resp1.status(), 201);

    // Duplicate slug
    let req2 = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "name": "Second",
            "slug": "duplicate-slug", "admin_password": "initial-strong-password"
        }))
        .to_request();
    let resp2 = test::call_service(&app, req2).await;
    assert!(
        resp2.status().is_client_error(),
        "Expected client error for duplicate slug"
    );
}

#[actix_web::test]
async fn list_tenants_returns_only_current_tenant() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tenants").route("", web::get().to(tenants::list))),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/tenants")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
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
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tenants").route("", web::post().to(tenants::create))),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "slug": "only-slug"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Missing name should return 400");
}

#[actix_web::test]
async fn create_tenant_missing_slug_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tenants").route("", web::post().to(tenants::create))),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "name": "Only Name"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Missing slug should return 400");
}

#[actix_web::test]
async fn create_tenant_empty_body_fails() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = provisioner_token(&ctx).await;

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(tenant_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tenants").route("", web::post().to(tenants::create))),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/tenants")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({}))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "Empty body should return 400");
}

async fn provisioner_token(ctx: &common::TestBackend) -> String {
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let admin = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    let id = ctx
        .role_repo
        .create_with_permissions(&dts_backend::types::CreateRoleParams {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            name: "provisioner".into(),
            description: String::new(),
            is_system: false,
            permissions: vec!["tenant:create".into()],
        })
        .await
        .unwrap();
    ctx.user_repo.assign_role(&admin.id, &id).await.unwrap();
    common::admin_token(ctx).await
}
