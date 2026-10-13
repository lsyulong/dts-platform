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

//! Health check API integration tests

use actix_web::{App, test, web};
use dts_server::app;
use pretty_assertions::assert_eq;

#[actix_web::test]
async fn health_check_returns_200_with_healthy_status() {
    let app =
        test::init_service(App::new().route("/health", web::get().to(app::health_check))).await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn health_check_returns_correct_json_structure() {
    let app =
        test::init_service(App::new().route("/health", web::get().to(app::health_check))).await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;

    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "dts-platform");
    assert!(body["version"].is_string());
    assert!(!body["version"].as_str().unwrap().is_empty());
}

#[actix_web::test]
async fn health_check_content_type_is_json() {
    let app =
        test::init_service(App::new().route("/health", web::get().to(app::health_check))).await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("application/json"));
}

#[actix_web::test]
async fn health_check_post_method_returns_405() {
    let app =
        test::init_service(App::new().route("/health", web::get().to(app::health_check))).await;

    let req = test::TestRequest::post().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 404, "POST to GET-only route returns 404");
}
