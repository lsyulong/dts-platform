//! AI Chat API integration tests
//!
//! Tests the SSE streaming AI chat endpoint behavior.

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
    let app = test::init_service(App::new().app_data(web::Data::new(ai_svc)).service(
        web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
    ))
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
    let mut config = DeerFlowConfig::default();
    config.enabled = false;
    config.api_key = "unused".to_string();
    let ai_svc = AiService::new(&config);
    let app = test::init_service(App::new().app_data(web::Data::new(ai_svc)).service(
        web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
    ))
    .await;

    // Generate a test JWT token
    use dts_server::services::token::{Claims, create_token};
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        username: "testuser".to_string(),
        exp: (now + chrono::Duration::hours(1)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    let token = create_token(&claims, "dev-secret-change-me").expect("test token creation failed");

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
    let mut config = DeerFlowConfig::default();
    config.enabled = true;
    config.api_key = String::new();
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
    let _cloned = svc.clone();
}

// ============================================================================
// Response format tests
// ============================================================================

#[actix_web::test]
async fn ai_chat_disabled_returns_error_json_body() {
    let config = DeerFlowConfig::default();
    let ai_svc = AiService::new(&config);
    let app = test::init_service(App::new().app_data(web::Data::new(ai_svc)).service(
        web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
    ))
    .await;

    use dts_server::services::token::{Claims, create_token};
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        username: "testuser".to_string(),
        exp: (now + chrono::Duration::hours(1)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    let token = create_token(&claims, "dev-secret-change-me").expect("test token creation failed");

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
    let app = test::init_service(App::new().app_data(web::Data::new(ai_svc)).service(
        web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
    ))
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
    let mut config = DeerFlowConfig::default();
    config.enabled = true;
    config.api_key = "".to_string();
    let ai_svc = AiService::new(&config);
    let app = test::init_service(App::new().app_data(web::Data::new(ai_svc)).service(
        web::scope("/api/v1/ai").route("/chat", web::post().to(dts_server::api::ai::chat)),
    ))
    .await;

    use dts_server::services::token::{Claims, create_token};
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        username: "testuser".to_string(),
        exp: (now + chrono::Duration::hours(1)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    let token = create_token(&claims, "dev-secret-change-me").expect("test token creation failed");

    let req = test::TestRequest::post()
        .uri("/api/v1/ai/chat?message=hello")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    // enabled=true but empty api_key → treated as disabled
    assert_eq!(resp.status(), 400);
}
