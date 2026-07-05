//! 认证流程集成测试 — 端到端登录、JWT 验证、权限检查

mod common;

use actix_web::{test, web, App};
use dts_server::api::auth;
use dts_server::services::auth_svc::AuthService;
use serde_json::json;

#[actix_web::test]
async fn login_with_valid_credentials_returns_token_and_user() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/login", web::post().to(auth::login)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(&json!({
            "username": "admin",
            "password": "admin123",
            "tenant_slug": "default"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["token"].is_string());
    assert!(!body["token"].as_str().unwrap().is_empty());
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["tenant_name"], "默认租户");
    assert!(body["user"]["roles"].is_array());
    assert!(body["user"]["permissions"].is_array());
}

#[actix_web::test]
async fn login_with_wrong_password_returns_401() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/login", web::post().to(auth::login)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(&json!({
            "username": "admin",
            "password": "wrong-password",
            "tenant_slug": "default"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn login_with_nonexistent_tenant_returns_401() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/login", web::post().to(auth::login)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(&json!({
            "username": "admin",
            "password": "admin123",
            "tenant_slug": "non-existent"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn me_endpoint_with_valid_token_returns_user_info() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let token = common::get_admin_token(&pool).await;
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/me", web::get().to(auth::me)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["username"], "admin");
    assert_eq!(body["tenant_name"], "默认租户");
}

#[actix_web::test]
async fn me_endpoint_without_token_returns_401() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/me", web::get().to(auth::me)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn me_endpoint_with_invalid_token_returns_401() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/me", web::get().to(auth::me)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Authorization", "Bearer invalid.token.here"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn permissions_endpoint_returns_permission_list() {
    let pool = common::setup_test_db_with_seed().await.unwrap();
    let config = common::test_config();
    let token = common::get_admin_token(&pool).await;
    let auth_svc = AuthService::new(pool.clone(), config.clone());

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::scope("/auth")
                            .route("/permissions", web::get().to(auth::my_permissions)),
                    ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/permissions")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["permissions"].is_array());
    assert!(!body["permissions"].as_array().unwrap().is_empty());
}
