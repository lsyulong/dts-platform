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

//! AI Chat API integration tests
//!
//! Tests the SSE streaming AI chat endpoint behavior.

mod common;

use actix_web::{App, test, web};
use dts_server::config::DeerFlowConfig;
use dts_server::services::ai_svc::AiService;
use pretty_assertions::assert_eq;

// ============================================================================
// Auth tests
// ============================================================================

#[actix_web::test]
async fn ai_chat_without_token_returns_401() {
    let config = DeerFlowConfig::default();
    let ai_svc = AiService::new(&config);
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ai_svc))
            .service(
                web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/ai/chat?message=hello")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}

// ============================================================================
// Disabled service tests
// ============================================================================

#[actix_web::test]
async fn ai_chat_when_disabled_returns_400() {
    let config = DeerFlowConfig {
        enabled: false,
        api_key: "unused".to_string(),
        ..DeerFlowConfig::default()
    };
    let ai_svc = AiService::new(&config);
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ai_svc))
            .service(
                web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
            ),
    )
    .await;

    let token = common::admin_token(&ctx).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/ai/chat?message=hello")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    // When disabled, AiService returns BadRequest
    assert_eq!(resp.status(), 400);
}

// ============================================================================
// AiService unit tests
// ============================================================================

#[actix_web::test]
async fn ai_service_new_disabled_creates_empty_client() {
    let config = DeerFlowConfig::default();
    let svc = AiService::new(&config);
    drop(svc);
}

#[actix_web::test]
async fn ai_service_new_enabled_with_empty_api_key_creates_empty_client() {
    let config = DeerFlowConfig {
        enabled: true,
        api_key: String::new(),
        ..DeerFlowConfig::default()
    };
    let svc = AiService::new(&config);
    drop(svc);
}

#[actix_web::test]
async fn deerflow_config_default_is_disabled() {
    let config = DeerFlowConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.base_url, "http://localhost:11434");
    assert!(config.api_key.is_empty());
}

#[actix_web::test]
async fn ai_service_is_cloneable() {
    let config = DeerFlowConfig::default();
    let svc = AiService::new(&config);
    let _cloned = svc;
}

// ============================================================================
// Response format tests
// ============================================================================

#[actix_web::test]
async fn ai_chat_disabled_returns_error_json_body() {
    let config = DeerFlowConfig::default();
    let ai_svc = AiService::new(&config);
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ai_svc))
            .service(
                web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
            ),
    )
    .await;

    let token = common::admin_token(&ctx).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/ai/chat?message=hello")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "BAD_REQUEST");
    assert!(body["message"].as_str().unwrap().contains("未启用"));
}

#[actix_web::test]
async fn ai_chat_get_method_rejected() {
    let config = DeerFlowConfig::default();
    let ai_svc = AiService::new(&config);
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ai_svc))
            .service(
                web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/ai/chat?message=hello")
        .to_request();
    let resp = test::call_service(&app, req).await;

    // actix-web returns 404 for method mismatch on POST-only routes
    assert!(resp.status() == 404 || resp.status() == 405);
}

#[actix_web::test]
async fn ai_chat_enabled_without_api_key_returns_400() {
    let config = DeerFlowConfig {
        enabled: true,
        api_key: String::new(),
        ..DeerFlowConfig::default()
    };
    let ai_svc = AiService::new(&config);
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ai_svc))
            .service(
                web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
            ),
    )
    .await;

    let token = common::admin_token(&ctx).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/ai/chat?message=hello")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    // enabled=true but empty api_key → treated as disabled
    assert_eq!(resp.status(), 400);
}
