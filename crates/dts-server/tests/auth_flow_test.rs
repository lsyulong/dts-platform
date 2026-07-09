//! Authentication flow integration tests — end-to-end login, JWT validation, permissions

mod common;

use actix_web::{test, web, App};
use dts_server::api::auth;
use dts_server::services::auth_svc::AuthService;
use dts_server::services::token::Claims;
use pretty_assertions::assert_eq;
use serde_json::json;

/// Get the Claims for the seed admin user (used to generate tokens).
async fn get_admin_claims(ctx: &dts_server::db::BackendContext) -> Claims {
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .expect("seed tenant")
        .expect("seed tenant should exist");

    let user = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .expect("seed user lookup")
        .expect("seed user should exist");

    Claims {
        sub: user.id,
        tenant_id: tenant.id,
        username: user.username,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        iat: chrono::Utc::now().timestamp() as usize,
    }
}

// ========== Login Tests ==========

#[actix_web::test]
async fn login_with_valid_credentials_returns_token_and_user() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1").service(
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
    assert!(body["user"]["tenant_name"].is_string());
    assert!(body["user"]["roles"].is_array());
    assert!(body["user"]["permissions"].is_array());
}

#[actix_web::test]
async fn login_with_wrong_password_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1").service(
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
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1").service(
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
async fn login_with_nonexistent_user_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/auth")
                        .route("/login", web::post().to(auth::login)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(&json!({
            "username": "nobody",
            "password": "admin123",
            "tenant_slug": "default"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn login_with_missing_field_returns_400() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .service(
                web::scope("/api/v1").service(
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
            "password": "admin123"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ========== Me Endpoint Tests ==========

#[actix_web::test]
async fn me_endpoint_with_valid_token_returns_user_info() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let claims = get_admin_claims(&ctx).await;
    let token =
        dts_server::services::token::create_token(&claims, common::DEFAULT_JWT_SECRET).unwrap();

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/auth").route("/me", web::get().to(auth::me))),
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
    assert!(body["tenant_name"].is_string());
}

#[actix_web::test]
async fn me_endpoint_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/auth").route("/me", web::get().to(auth::me))),
            ),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/auth/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn me_endpoint_with_invalid_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/auth").route("/me", web::get().to(auth::me))),
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

// ========== Permissions Endpoint Tests ==========

#[actix_web::test]
async fn permissions_endpoint_returns_permission_list() {
    let ctx = common::setup_test_backend().await.expect("backend init");
    let claims = get_admin_claims(&ctx).await;
    let token =
        dts_server::services::token::create_token(&claims, common::DEFAULT_JWT_SECRET).unwrap();

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1").service(
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
}

#[actix_web::test]
async fn permissions_endpoint_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        common::DEFAULT_JWT_SECRET.to_string(),
        24,
    );

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1").service(
                    web::scope("/auth")
                        .route("/permissions", web::get().to(auth::my_permissions)),
                ),
            ),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/permissions")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
