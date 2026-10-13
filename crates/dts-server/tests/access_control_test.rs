mod common;

use actix_web::{App, test, web};
use dts_backend::types::{CreateTenantParams, CreateUserParams};
use dts_server::api::{system, tenants};
use dts_server::services::{tenant_svc::TenantService, user_svc::UserService};
use serde_json::json;

#[actix_web::test]
async fn users_without_manage_permission_cannot_list_users() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    ctx.user_repo
        .create(&CreateUserParams {
            id: "reader".into(),
            tenant_id: tenant.id.clone(),
            username: "reader".into(),
            email: "reader@example.test".into(),
            password_hash: "unused".into(),
            display_name: None,
        })
        .await
        .unwrap();
    let auth = common::test_auth_service(&ctx);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth))
            .app_data(web::Data::new(UserService::new(ctx.user_repo.clone())))
            .route("/users", web::get().to(system::list_users)),
    )
    .await;
    let token = common::test_token(&ctx, "reader").await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
}

#[actix_web::test]
async fn tenant_admin_list_is_scoped_and_cannot_reset_another_tenants_password() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let admin = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    ctx.tenant_repo
        .create(&CreateTenantParams {
            id: "other".into(),
            name: "Other".into(),
            slug: "other".into(),
        })
        .await
        .unwrap();
    ctx.user_repo
        .create(&CreateUserParams {
            id: "other-user".into(),
            tenant_id: "other".into(),
            username: "other-user".into(),
            email: "other@example.test".into(),
            password_hash: "must-not-change".into(),
            display_name: None,
        })
        .await
        .unwrap();
    let auth = common::test_auth_service(&ctx);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth))
            .app_data(web::Data::new(UserService::new(ctx.user_repo.clone())))
            .route("/users", web::get().to(system::list_users))
            .route(
                "/users/{id}/reset-password",
                web::post().to(system::reset_password),
            ),
    )
    .await;
    let token = common::test_token(&ctx, &admin.id).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert!(
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u["tenant_id"] == tenant.id)
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/users/other-user/reset-password")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"new_password":"changed-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
    let other = ctx
        .user_repo
        .find_by_tenant_and_username("other", "other-user")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.password_hash, "must-not-change");
}

#[actix_web::test]
async fn anonymous_tenant_creation_is_rejected() {
    let ctx = common::setup_test_backend().await.unwrap();
    let auth = common::test_auth_service(&ctx);
    let svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth))
            .app_data(web::Data::new(svc))
            .route("/tenants", web::post().to(tenants::create)),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/tenants")
            .set_json(json!({"name":"Anonymous", "slug":"anonymous"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 401);
}

#[actix_web::test]
async fn disabled_accounts_lose_access_with_an_existing_token() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = ctx
        .tenant_repo
        .find_by_slug("default")
        .await
        .unwrap()
        .unwrap();
    let admin = ctx
        .user_repo
        .find_by_tenant_and_username(&tenant.id, "admin")
        .await
        .unwrap()
        .unwrap();
    let auth = common::test_auth_service(&ctx);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth))
            .app_data(web::Data::new(UserService::new(ctx.user_repo.clone())))
            .route("/users", web::get().to(system::list_users)),
    )
    .await;
    let token = common::admin_token(&ctx).await;
    ctx.user_repo
        .update(
            &admin.id,
            &dts_backend::types::UpdateUserParams {
                display_name: None,
                status: Some("disabled".into()),
            },
        )
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/users")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 401);
}

#[actix_web::test]
async fn query_endpoint_rejects_a_different_tenant_before_connecting() {
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(dts_server::api::query::QueryAccess {
                tenant_id: Some("another-tenant".into()),
            }))
            .app_data(web::Data::new(
                dts_server::services::query_svc::QueryService::new(
                    &dts_server::config::SwanLakeConfig::default(),
                ),
            ))
            .route("/query", web::post().to(dts_server::api::query::execute)),
    )
    .await;
    let token = common::admin_token(&ctx).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/query")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"sql":"SELECT 1"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
}

#[actix_web::test]
async fn websocket_auth_and_ownership_are_checked_before_upgrade() {
    let ctx = common::setup_test_backend().await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(ctx.task_repo.clone()))
            .app_data(web::Data::new(dts_server::ws::task_log::WsHub::new()))
            .route(
                "/ws/{id}",
                web::get().to(dts_server::ws::task_log::task_log_ws),
            ),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/ws/not-owned").to_request(),
    )
    .await;
    assert_eq!(response.status(), 401);
    let token = common::admin_token(&ctx).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/ws/not-owned")
            .insert_header((
                "Sec-WebSocket-Protocol",
                format!("dts.logs, bearer.{token}"),
            ))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
}
