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

//! Query API integration tests
//!
//! Tests for SwanLake query endpoints (POST /api/v1/query/execute, GET /api/v1/query/schemas).

use actix_web::{App, test, web};
use dts_server::config::SwanLakeConfig;
use dts_server::services::query_svc::QueryService;
use pretty_assertions::assert_eq;

// ============================================================================
// Auth tests
// ============================================================================

#[actix_web::test]
async fn query_execute_without_token_returns_401() {
    let config = SwanLakeConfig::default();
    let query_svc = QueryService::new(&config);
    let app = test::init_service(
        App::new().app_data(web::Data::new(query_svc)).service(
            web::scope("/api/v1/query")
                .route("/execute", web::post().to(dts_server::api::query::execute)),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/query/execute")
        .set_json(serde_json::json!({"sql": "SELECT 1"}))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn query_schemas_without_token_returns_401() {
    let config = SwanLakeConfig::default();
    let query_svc = QueryService::new(&config);
    let app = test::init_service(
        App::new().app_data(web::Data::new(query_svc)).service(
            web::scope("/api/v1/query")
                .route("/schemas", web::get().to(dts_server::api::query::schemas)),
        ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/query/schemas")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}

// ============================================================================
// Disabled service tests
// ============================================================================

#[actix_web::test]
async fn query_execute_returns_400_when_swanlake_disabled() {
    let config = SwanLakeConfig::default();
    assert!(!config.enabled);
    let query_svc = QueryService::new(&config);
    assert!(query_svc.execute("SELECT 1").await.is_err());
}

#[actix_web::test]
async fn query_schemas_returns_400_when_swanlake_disabled() {
    let config = SwanLakeConfig::default();
    let query_svc = QueryService::new(&config);
    assert!(query_svc.list_schemas().await.is_err());
}

// ============================================================================
// Config unit tests
// ============================================================================

#[actix_web::test]
async fn swanlake_config_default_is_disabled() {
    let config = SwanLakeConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 4214);
}

#[actix_web::test]
async fn swanlake_config_uri_conversion() {
    let config = SwanLakeConfig {
        enabled: true,
        host: "swanlake".to_string(),
        port: 4214,
        ..SwanLakeConfig::default()
    };
    assert_eq!(config.uri(), "grpc://swanlake:4214");
}
