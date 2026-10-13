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

//! Authentication flow integration tests — end-to-end login, Sa-Token sessions, permissions

mod common;

use actix_web::{App, test, web};
use dts_server::api::auth;
use pretty_assertions::assert_eq;
use serde_json::json;

// ========== Login Tests ==========

#[actix_web::test]
async fn login_with_valid_credentials_returns_token_and_user() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(
                web::scope("/api/v1")
                    .service(web::scope("/auth").route("/login", web::post().to(auth::login))),
            ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(json!({
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

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new().app_data(web::Data::new(auth_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/auth").route("/login", web::post().to(auth::login))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(json!({
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

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new().app_data(web::Data::new(auth_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/auth").route("/login", web::post().to(auth::login))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(json!({
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

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new().app_data(web::Data::new(auth_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/auth").route("/login", web::post().to(auth::login))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(json!({
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

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new().app_data(web::Data::new(auth_svc)).service(
            web::scope("/api/v1")
                .service(web::scope("/auth").route("/login", web::post().to(auth::login))),
        ),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(json!({
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
    let token = common::admin_token(&ctx).await;

    let auth_svc = common::test_auth_service(&ctx);

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
        .insert_header(("Authorization", format!("Bearer {token}")))
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

    let auth_svc = common::test_auth_service(&ctx);

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

    let auth_svc = common::test_auth_service(&ctx);

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
    let token = common::admin_token(&ctx).await;

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(web::scope("/api/v1").service(
                web::scope("/auth").route("/permissions", web::get().to(auth::my_permissions)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/permissions")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["permissions"].is_array());
}

#[actix_web::test]
async fn permissions_endpoint_without_token_returns_401() {
    let ctx = common::setup_test_backend().await.expect("backend init");

    let auth_svc = common::test_auth_service(&ctx);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_svc))
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .service(web::scope("/api/v1").service(
                web::scope("/auth").route("/permissions", web::get().to(auth::my_permissions)),
            )),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/permissions")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}
