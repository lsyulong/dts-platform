// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use actix_web::{App, http::StatusCode, test, web};
use dts_backend::types::UpdateUserParams;
use dts_server::api::{auth, system};
use dts_server::services::{auth_svc::AuthService, user_svc::UserService};
use sa_token_core::TokenValue;
use serde_json::{Value, json};

fn configure_api(ctx: &common::TestBackend) -> impl FnOnce(&mut web::ServiceConfig) {
    let auth_service = common::test_auth_service(ctx);
    let users = ctx.user_repo.clone();
    let roles = ctx.role_repo.clone();
    let user_service = UserService::new(users.clone());
    move |config| {
        config
            .app_data(web::Data::new(auth_service))
            .app_data(web::Data::new(users))
            .app_data(web::Data::new(roles))
            .app_data(web::Data::new(user_service))
            .service(
                web::scope("/api/v1")
                    .route("/auth/login", web::post().to(auth::login))
                    .route("/auth/logout", web::post().to(auth::logout))
                    .route("/auth/me", web::get().to(auth::me))
                    .route("/system/users/{id}", web::put().to(system::update_user))
                    .route(
                        "/system/users/{id}/reset-password",
                        web::post().to(system::reset_password),
                    ),
            );
    }
}

#[actix_web::test]
async fn logout_revokes_the_real_session_across_app_instances_sharing_the_manager() {
    let ctx = common::setup_test_backend().await.unwrap();
    let first_app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let second_app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let response = test::call_service(
        &first_app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"admin", "password":"admin123"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    let token = login["token"].as_str().unwrap();

    let response = test::call_service(
        &second_app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = test::call_service(
        &first_app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = test::call_service(
        &second_app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_unregistered_random_and_genuinely_expired_session_tokens() {
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let random_token = "a-realistic-format-but-unregistered-random-token-000000000000000000";
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {random_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let expired_token = common::expired_admin_token(&ctx).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {expired_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn password_reset_revokes_all_old_sessions_and_accepts_only_the_new_password() {
    let ctx = common::setup_test_backend().await.unwrap();
    let first_token = common::admin_token(&ctx).await;
    let second_token = common::admin_token(&ctx).await;
    assert_ne!(
        first_token, second_token,
        "Concurrent logins must have distinct sessions"
    );
    let identity = ctx.auth.authenticate(&first_token).await.unwrap();
    let app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/system/users/{}/reset-password",
                identity.sub
            ))
            .insert_header(("Authorization", format!("Bearer {first_token}")))
            .set_json(json!({"new_password":"synthetic-new-admin-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    for token in [&first_token, &second_token] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/auth/me")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"admin", "password":"admin123"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"admin",
            "password":"synthetic-new-admin-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    let new_token = login["token"].as_str().unwrap();
    assert_ne!(new_token, first_token);
    assert_ne!(new_token, second_token);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {new_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[actix_web::test]
async fn changing_the_stored_password_rejects_a_still_registered_old_session() {
    let ctx = common::setup_test_backend().await.unwrap();
    let token = common::admin_token(&ctx).await;
    let identity = ctx.auth.authenticate(&token).await.unwrap();
    let hash = AuthService::hash_password_sync("synthetic-out-of-band-password").unwrap();
    // Model a password update whose subsequent storage revocation did not run.
    ctx.user_repo
        .reset_password(&identity.sub, &hash)
        .await
        .unwrap();
    assert!(
        ctx.auth
            .manager
            .is_valid(&TokenValue::new(token.clone()))
            .await,
        "This regression must exercise a live token whose credential version became stale"
    );
    let app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn reactivating_an_account_revokes_residual_sessions_before_enabling_new_logins() {
    let ctx = common::setup_test_backend().await.unwrap();
    let caller_token = common::admin_token(&ctx).await;
    let caller = ctx.auth.authenticate(&caller_token).await.unwrap();
    let target = UserService::new(ctx.user_repo.clone())
        .create_user(
            &caller.tenant_id,
            "restored-user",
            "restored@example.invalid",
            "synthetic-restored-password",
            None,
        )
        .await
        .unwrap();
    let old_token = common::test_token(&ctx, &target.id).await;
    assert!(ctx.auth.authenticate(&old_token).await.is_ok());
    // A database-only stop models a failed or partial external session revocation.
    ctx.user_repo
        .update(
            &target.id,
            &UpdateUserParams {
                display_name: None,
                status: Some("disabled".into()),
            },
        )
        .await
        .unwrap();
    assert!(
        ctx.auth
            .manager
            .is_valid(&TokenValue::new(old_token.clone()))
            .await
    );
    let app = test::init_service(App::new().configure(configure_api(&ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {old_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/v1/system/users/{}", target.id))
            .insert_header(("Authorization", format!("Bearer {caller_token}")))
            .set_json(json!({"status":"active"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {old_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Reactivation must not revive a residual session"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"restored-user",
                "password":"synthetic-restored-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    let new_token = login["token"].as_str().unwrap();
    assert_ne!(new_token, old_token);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {new_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[actix_web::test]
async fn editing_an_already_active_account_or_self_preserves_existing_sessions() {
    let ctx = common::setup_test_backend().await.unwrap();
    let caller_token = common::admin_token(&ctx).await;
    let caller = ctx.auth.authenticate(&caller_token).await.unwrap();
    let target = UserService::new(ctx.user_repo.clone())
        .create_user(
            &caller.tenant_id,
            "active-peer",
            "active-peer@example.invalid",
            "synthetic-active-peer-password",
            None,
        )
        .await
        .unwrap();
    let roles = ctx
        .role_repo
        .list_all(Some(&caller.tenant_id))
        .await
        .unwrap();
    let admin_role = roles
        .iter()
        .find(|role| role.name == "tenant_admin")
        .unwrap();
    ctx.user_repo
        .assign_role(&target.id, &admin_role.id)
        .await
        .unwrap();
    let peer_token = common::test_token(&ctx, &target.id).await;
    let app = test::init_service(App::new().configure(configure_api(&ctx))).await;

    for (user_id, token, name) in [
        (&target.id, &peer_token, "Edited active peer"),
        (&caller.sub, &caller_token, "Edited own account"),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!("/api/v1/system/users/{user_id}"))
                .insert_header(("Authorization", format!("Bearer {caller_token}")))
                .set_json(json!({"display_name":name, "status":"active"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/auth/me")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "An active account's ordinary edits must not log it out"
        );
        let profile: Value = test::read_body_json(response).await;
        assert_eq!(profile["display_name"], name);
    }
}
