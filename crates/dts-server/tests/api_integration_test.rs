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

//! API handler integration tests — HTTP-level tests for authenticated endpoints
//!
//! Uses SQLite in-memory backend with seed data, then tests:
//! - Datasource listing (authenticated)
//! - Task listing (authenticated)
//! - Unauthenticated access to protected endpoints

mod common;

use actix_web::{App, test, web};
use dts_server::api::{datasources, tasks};
use dts_server::services::datasource_svc::DatasourceService;
use dts_server::services::task_svc::TaskService;
use pretty_assertions::assert_eq;

// ========== Datasource Endpoint Tests ==========

#[actix_web::test]
async fn list_datasources_with_valid_token_returns_data() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = common::admin_token(&ctx).await;

    let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());

    let app =
        test::init_service(
            App::new()
                .app_data(web::Data::new(common::test_auth_service(&ctx)))
                .app_data(web::Data::new(datasource_svc))
                .service(web::scope("/api/v1").service(
                    web::scope("/datasources").route("", web::get().to(datasources::list)),
                )),
        )
        .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/datasources")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2, "Should have 2 seeded datasources");
}

#[actix_web::test]
async fn list_datasources_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());

    let app =
        test::init_service(
            App::new()
                .app_data(web::Data::new(common::test_auth_service(&ctx)))
                .app_data(web::Data::new(datasource_svc))
                .service(web::scope("/api/v1").service(
                    web::scope("/datasources").route("", web::get().to(datasources::list)),
                )),
        )
        .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/datasources")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        401,
        "Unauthenticated request should return 401"
    );
}

#[actix_web::test]
async fn list_datasources_with_invalid_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());

    let app =
        test::init_service(
            App::new()
                .app_data(web::Data::new(common::test_auth_service(&ctx)))
                .app_data(web::Data::new(datasource_svc))
                .service(web::scope("/api/v1").service(
                    web::scope("/datasources").route("", web::get().to(datasources::list)),
                )),
        )
        .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/datasources")
        .insert_header(("Authorization", "Bearer invalid.token.here"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn list_datasources_with_expired_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let expired_token = common::expired_admin_token(&ctx).await;

    let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());

    let app =
        test::init_service(
            App::new()
                .app_data(web::Data::new(common::test_auth_service(&ctx)))
                .app_data(web::Data::new(datasource_svc))
                .service(web::scope("/api/v1").service(
                    web::scope("/datasources").route("", web::get().to(datasources::list)),
                )),
        )
        .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/datasources")
        .insert_header(("Authorization", format!("Bearer {expired_token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// ========== Task Endpoint Tests ==========

#[actix_web::test]
async fn list_tasks_with_valid_token_returns_data() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = common::admin_token(&ctx).await;

    let task_svc = TaskService::new(ctx.task_repo.clone(), ctx.datasource_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(task_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tasks").route("", web::get().to(tasks::list))),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/tasks")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    // No tasks seeded by default, so data should be empty array
    let total = body["total"].as_i64().unwrap_or(-1);
    assert_eq!(total, 0, "No tasks should exist initially");
}

#[actix_web::test]
async fn list_tasks_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let task_svc = TaskService::new(ctx.task_repo.clone(), ctx.datasource_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(task_svc))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/tasks").route("", web::get().to(tasks::list))),
            ),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/tasks").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status(),
        401,
        "Unauthenticated request should return 401"
    );
}

// ========== Auth Data Test (verify seed data is usable) ==========

#[actix_web::test]
async fn login_response_contains_all_user_fields() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(auth_svc))
            .service(web::scope("/api/v1").service(
                web::scope("/auth").route("/login", web::post().to(dts_server::api::auth::login)),
            )),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({
            "username": "admin",
            "password": "admin123",
            "tenant_slug": "default"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;

    // Verify all expected fields are present
    assert!(body["token"].is_string());
    assert!(!body["token"].as_str().unwrap().is_empty());

    let user = &body["user"];
    assert!(user["id"].is_string());
    assert_eq!(user["username"], "admin");
    assert_eq!(user["email"], "admin@dts.local");
    assert!(user["tenant_id"].is_string());
    assert!(user["tenant_name"].is_string());
    assert!(user["roles"].is_array());
    assert!(user["permissions"].is_array());
}
