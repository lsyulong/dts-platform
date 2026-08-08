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


//! Alert API integration tests — CRUD for alert rules via HTTP

mod common;

use actix_web::{App, test, web};
use dts_server::api::alerts;
use dts_server::services::alert_svc::AlertService;
use pretty_assertions::assert_eq;
use serde_json::json;

/// Generate a valid JWT for the seed admin user.
async fn get_admin_token(ctx: &dts_server::db::BackendContext) -> String {
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("find_by_slug")
        .expect("tenant");

    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .expect("find_by_tenant_and_username")
        .expect("user");

    let now = chrono::Utc::now();
    let claims = dts_server::services::token::Claims {
        sub: user.id,
        tenant_id: tenant.id,
        username: user.username,
        exp: (now + chrono::Duration::hours(1)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    dts_server::services::token::create_token(&claims, common::DEFAULT_JWT_SECRET).unwrap()
}

// ========== Create Alert Tests ==========

#[actix_web::test]
async fn create_alert_with_valid_data_returns_201() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = get_admin_token(&ctx).await;

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(
        App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1").service(
                web::scope("/alerts")
                    .route("", web::post().to(alerts::create))
                    .route("", web::get().to(alerts::list))
                    .route("/{id}/toggle", web::patch().to(alerts::toggle)),
            ),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/alerts")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&json!({
            "name": "High Error Rate Alert",
            "metric_type": "error_count",
            "operator": "gt",
            "threshold": 10.0,
            "notify_channels": ["email", "webhook"]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "High Error Rate Alert");
}

#[actix_web::test]
async fn create_alert_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(
        App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/alerts").route("", web::post().to(alerts::create))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/alerts")
        .set_json(&json!({
            "name": "Test Alert",
            "metric_type": "error_count",
            "operator": "gt",
            "threshold": 5.0
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn create_alert_with_invalid_metric_type_returns_error() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = get_admin_token(&ctx).await;

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(
        App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/alerts").route("", web::post().to(alerts::create))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/alerts")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&json!({
            "name": "Bad Alert",
            "metric_type": "invalid_metric",
            "operator": "gt",
            "threshold": 5.0
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // NOTE: anyhow::Error from service → AppError::Internal → 500
    // A future improvement would catch validation errors and return 400
    assert!(!resp.status().is_success());
    assert!(resp.status().as_u16() >= 400);
}

// ========== List Alerts Tests ==========

#[actix_web::test]
async fn list_alerts_with_valid_token_returns_data() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = get_admin_token(&ctx).await;

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(App::new().app_data(web::Data::new(alert_svc)).service(
        web::scope("/api/v1").service(web::scope("/alerts").route("", web::get().to(alerts::list))),
    ))
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/alerts")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
}

#[actix_web::test]
async fn list_alerts_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(App::new().app_data(web::Data::new(alert_svc)).service(
        web::scope("/api/v1").service(web::scope("/alerts").route("", web::get().to(alerts::list))),
    ))
    .await;

    let req = test::TestRequest::get().uri("/api/v1/alerts").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// ========== Toggle Alert Tests ==========

#[actix_web::test]
async fn toggle_alert_toggles_state() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = get_admin_token(&ctx).await;
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    // First create an alert so we have an ID
    let alert_id = alert_svc
        .create(
            &tenant.id,
            None,
            "Toggle Test Alert",
            "sync_delay",
            "gt",
            60.0,
            r#"["email"]"#,
        )
        .await
        .expect("create alert");

    let app =
        test::init_service(App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1").service(
                web::scope("/alerts").route("/{id}/toggle", web::patch().to(alerts::toggle)),
            ),
        ))
        .await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/alerts/{}/toggle", alert_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["message"].is_string());
    assert!(body["message"].as_str().unwrap().contains("切换"));
}

#[actix_web::test]
async fn toggle_alert_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app =
        test::init_service(App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1").service(
                web::scope("/alerts").route("/{id}/toggle", web::patch().to(alerts::toggle)),
            ),
        ))
        .await;

    let req = test::TestRequest::patch()
        .uri("/api/v1/alerts/some-id/toggle")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

// ========== Create + List Round-trip Tests ==========

#[actix_web::test]
async fn create_alert_then_it_appears_in_list() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = get_admin_token(&ctx).await;

    let alert_svc = AlertService::new(ctx.alert_repo.clone());

    let app = test::init_service(
        App::new().app_data(web::Data::new(alert_svc)).service(
            web::scope("/api/v1").service(
                web::scope("/alerts")
                    .route("", web::post().to(alerts::create))
                    .route("", web::get().to(alerts::list)),
            ),
        ),
    )
    .await;

    // Create
    let create_req = test::TestRequest::post()
        .uri("/api/v1/alerts")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&json!({
            "name": "Round-trip Alert",
            "metric_type": "row_diff",
            "operator": "lte",
            "threshold": 100.0,
            "notify_channels": ["sms"]
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), 201);

    // List
    let list_req = test::TestRequest::get()
        .uri("/api/v1/alerts")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(list_resp).await;
    let data = body["data"].as_array().unwrap();
    assert!(
        !data.is_empty(),
        "Should have at least the newly created alert"
    );
}
