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

//! Monitoring API integration tests — dashboard overview endpoint

mod common;

use actix_web::{App, test, web};
use dts_server::api::monitoring;
use dts_server::services::monitoring_svc::MonitoringService;
use pretty_assertions::assert_eq;

// ========== Overview Tests ==========

#[actix_web::test]
async fn overview_with_valid_token_returns_200() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = common::admin_token(&ctx).await;

    let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(monitoring_svc))
            .service(web::scope("/api/v1").service(
                web::scope("/monitoring").route("/overview", web::get().to(monitoring::overview)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/monitoring/overview")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn overview_returns_correct_structure() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let token = common::admin_token(&ctx).await;

    let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(monitoring_svc))
            .service(web::scope("/api/v1").service(
                web::scope("/monitoring").route("/overview", web::get().to(monitoring::overview)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/monitoring/overview")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert!(
        body["total_tasks"].is_number(),
        "total_tasks should be a number"
    );
    assert!(
        body["running_tasks"].is_number(),
        "running_tasks should be a number"
    );
    assert!(
        body["failed_tasks"].is_number(),
        "failed_tasks should be a number"
    );
    assert!(
        body["total_datasources"].is_number(),
        "total_datasources should be a number"
    );
    assert!(
        body["healthy_datasources"].is_number(),
        "healthy_datasources should be a number"
    );
    assert!(
        body["today_synced_rows"].is_number(),
        "today_synced_rows should be a number"
    );
    assert!(
        body["today_synced_bytes"].is_number(),
        "today_synced_bytes should be a number"
    );
    assert!(
        body["success_rate"].is_number(),
        "success_rate should be a number"
    );
    assert!(
        body["recent_runs"].is_array(),
        "recent_runs should be an array"
    );
}

#[actix_web::test]
async fn overview_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(monitoring_svc))
            .service(web::scope("/api/v1").service(
                web::scope("/monitoring").route("/overview", web::get().to(monitoring::overview)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/monitoring/overview")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn overview_with_expired_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let expired_token = common::expired_admin_token(&ctx).await;

    let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(monitoring_svc))
            .service(web::scope("/api/v1").service(
                web::scope("/monitoring").route("/overview", web::get().to(monitoring::overview)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/monitoring/overview")
        .insert_header(("Authorization", format!("Bearer {expired_token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
