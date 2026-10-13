// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use actix_web::{App, http::StatusCode, test, web};
use dts_backend::types::CreateRoleParams;
use dts_server::api::{auth, system, tenants};
use dts_server::db;
use dts_server::services::{
    auth_svc::AuthService, sa_auth, tenant_svc::TenantService, user_svc::UserService,
};
use serde_json::{Value, json};

struct Fixture {
    ctx: common::TestBackend,
    tenant_id: String,
    admin_id: String,
    token: String,
}

impl Fixture {
    async fn new() -> Self {
        let ctx = common::setup_test_backend().await.expect("SQLite backend");
        Self::from_backend(ctx).await
    }

    async fn bootstrap() -> Self {
        let ctx = db::init_backend_with_bootstrap(
            "sqlite::memory:?cache=shared",
            "sqlite",
            Some("synthetic-platform-owner-password"),
        )
        .await
        .expect("Strict bootstrap SQLite backend");
        let manager = sa_auth::test_manager(ctx.role_repo.clone());
        let auth = AuthService::new(
            ctx.tenant_repo.clone(),
            ctx.user_repo.clone(),
            ctx.role_repo.clone(),
            manager,
        );
        Self::from_backend(common::TestBackend { ctx, auth }).await
    }

    async fn from_backend(ctx: common::TestBackend) -> Self {
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
        let token = common::admin_token(&ctx).await;
        Self {
            ctx,
            tenant_id: tenant.id,
            admin_id: admin.id,
            token,
        }
    }

    /// Provision a role independently of the HTTP authorization being tested.
    async fn provision_role(&self, name: &str, permissions: &[&str]) -> String {
        self.ctx
            .role_repo
            .create_with_permissions(&CreateRoleParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: self.tenant_id.clone(),
                name: name.into(),
                description: "Authorization fixture".into(),
                is_system: false,
                permissions: permissions
                    .iter()
                    .map(|permission| (*permission).into())
                    .collect(),
            })
            .await
            .unwrap()
    }
}

fn configure_api(ctx: &common::TestBackend) -> impl FnOnce(&mut web::ServiceConfig) {
    let auth_service = common::test_auth_service(ctx);
    let user_service = UserService::new(ctx.user_repo.clone());
    let tenant_service = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );
    let users = ctx.user_repo.clone();
    let roles = ctx.role_repo.clone();
    move |config| {
        config
            .app_data(web::Data::new(auth_service))
            .app_data(web::Data::new(user_service))
            .app_data(web::Data::new(tenant_service))
            .app_data(web::Data::new(users))
            .app_data(web::Data::new(roles))
            .service(
                web::scope("/api/v1")
                    .route("/system/roles", web::post().to(system::create_role))
                    .route("/system/roles", web::get().to(system::list_roles))
                    .route("/system/roles/{id}", web::put().to(system::update_role))
                    .route("/system/users", web::post().to(system::create_user))
                    .route("/system/users", web::get().to(system::list_users))
                    .route("/system/users/{id}", web::put().to(system::update_user))
                    .route("/system/users/{id}", web::delete().to(system::delete_user))
                    .route(
                        "/system/users/{id}/reset-password",
                        web::post().to(system::reset_password),
                    )
                    .route(
                        "/system/permissions",
                        web::get().to(system::list_permissions),
                    )
                    .route("/auth/permissions", web::get().to(auth::my_permissions))
                    .route("/auth/login", web::post().to(auth::login))
                    .route("/auth/me", web::get().to(auth::me))
                    .route("/tenants", web::post().to(tenants::create)),
            );
    }
}

#[actix_web::test]
async fn tenant_admin_cannot_create_a_role_with_platform_tenant_creation_permission() {
    let fixture = Fixture::new().await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"name":"platform-escalation", "permissions":["task:read", "tenant:create"]}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let roles: Value = test::read_body_json(response).await;
    assert!(
        !roles["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|role| role["name"] == "platform-escalation")
    );

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/tenants")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"name":"Unauthorized tenant", "slug":"unauthorized-tenant",
            "admin_password":"synthetic-initial-password"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn tenant_admin_cannot_add_platform_permission_to_an_existing_custom_role() {
    let fixture = Fixture::new().await;
    let role_id = fixture
        .provision_role("ordinary-role", &["task:read"])
        .await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(&app, test::TestRequest::put()
        .uri(&format!("/api/v1/system/roles/{role_id}"))
        .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
        .set_json(json!({"name":"partially-mutated-role", "permissions":["task:read", "tenant:create"]}))
        .to_request()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let roles: Value = test::read_body_json(response).await;
    let role = roles["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["id"] == role_id)
        .unwrap();
    assert_eq!(role["name"], "ordinary-role");
    assert_eq!(role["permissions"], json!(["task:read"]));
}

#[actix_web::test]
async fn tenant_admin_cannot_assign_a_preprovisioned_platform_role_to_self() {
    let fixture = Fixture::new().await;
    let role_id = fixture
        .provision_role("preprovisioned-platform-role", &["tenant:create"])
        .await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/v1/system/users/{}", fixture.admin_id))
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"display_name":"unauthorized-mutation", "role_ids":[role_id]}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/permissions")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let permissions: Value = test::read_body_json(response).await;
    let permissions = permissions["permissions"].as_array().unwrap();
    assert!(
        permissions.contains(&json!("user:manage")),
        "A rejected assignment must preserve existing roles"
    );
    assert!(!permissions.contains(&json!("tenant:create")));

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let users: Value = test::read_body_json(response).await;
    let admin = users["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["id"] == fixture.admin_id)
        .unwrap();
    assert_ne!(admin["display_name"], "unauthorized-mutation");
}

#[actix_web::test]
async fn tenant_admin_cannot_create_a_user_with_a_preprovisioned_platform_role() {
    let fixture = Fixture::new().await;
    let role_id = fixture
        .provision_role("preprovisioned-platform-role", &["tenant:create"])
        .await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"username":"unauthorized-provisioner", "email":"fixture@example.invalid",
            "password":"synthetic-initial-password", "role_ids":[role_id]}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let users: Value = test::read_body_json(response).await;
    assert!(
        !users["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|user| user["username"] == "unauthorized-provisioner")
    );
}

#[actix_web::test]
async fn tenant_admin_can_create_update_and_assign_roles_with_permissions_already_held() {
    let fixture = Fixture::new().await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"name":"delegated-reader", "permissions":["task:read"]}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = test::read_body_json(response).await;
    let role_id = created["id"].as_str().unwrap();

    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/v1/system/roles/{role_id}"))
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"permissions":["task:read", "datasource:read"]}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"username":"delegated-reader", "email":"reader@example.invalid",
            "password":"synthetic-initial-password", "role_ids":[role_id]}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = test::read_body_json(response).await;
    let reader_token = common::test_token(&fixture.ctx, created["id"].as_str().unwrap()).await;

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/permissions")
            .insert_header(("Authorization", format!("Bearer {reader_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let permissions: Value = test::read_body_json(response).await;
    let mut permissions: Vec<_> = permissions["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|permission| permission.as_str().unwrap())
        .collect();
    permissions.sort_unstable();
    assert_eq!(permissions, vec!["datasource:read", "task:read"]);
}

#[actix_web::test]
async fn tenant_admin_permission_picker_exposes_only_grantable_permissions() {
    let fixture = Fixture::new().await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/permissions")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let permissions: Value = test::read_body_json(response).await;
    let codes: Vec<_> = permissions["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|permission| permission["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"task:read"));
    assert!(codes.contains(&"user:manage"));
    assert!(!codes.contains(&"tenant:create"));
}

#[actix_web::test]
async fn delegated_tenant_admin_cannot_take_over_or_remove_the_platform_owner() {
    let fixture = Fixture::bootstrap().await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let roles: Value = test::read_body_json(response).await;
    let tenant_admin_role = roles["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["name"] == "tenant_admin")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"username":"delegated-admin", "email":"delegated@example.invalid",
                "password":"synthetic-delegated-password", "role_ids":[tenant_admin_role]}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(
                json!({"tenant_slug":"default", "username":"delegated-admin",
                "password":"synthetic-delegated-password"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    let token = login["token"].as_str().unwrap();
    let granted = login["user"]["permissions"].as_array().unwrap();
    assert!(granted.contains(&json!("user:manage")));
    assert!(!granted.contains(&json!("tenant:create")));

    let owner_uri = format!("/api/v1/system/users/{}", fixture.admin_id);
    let reset_uri = format!("{owner_uri}/reset-password");
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&reset_uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"new_password":"synthetic-takeover-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Stripping the target's higher role must not make a second reset legal.
    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&owner_uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"role_ids":[]}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&reset_uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"new_password":"synthetic-takeover-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&owner_uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"status":"disabled"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&owner_uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let owner: Value = test::read_body_json(response).await;
    assert!(
        owner["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("tenant:create"))
    );
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"admin",
                "password":"synthetic-platform-owner-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Denied mutations must preserve the owner's account and password"
    );
}

#[actix_web::test]
async fn tenant_admin_can_manage_a_user_with_equal_permissions() {
    let fixture = Fixture::new().await;
    let app = test::init_service(App::new().configure(configure_api(&fixture.ctx))).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/system/roles")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let roles: Value = test::read_body_json(response).await;
    let tenant_admin_role = roles["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["name"] == "tenant_admin")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/system/users")
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(
                json!({"username":"peer-admin", "email":"peer@example.invalid",
                "password":"synthetic-peer-password", "role_ids":[tenant_admin_role]}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let user: Value = test::read_body_json(response).await;
    let peer_id = user["id"].as_str().unwrap();
    let peer_uri = format!("/api/v1/system/users/{peer_id}");
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"peer-admin",
                "password":"synthetic-peer-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    let old_peer_token = login["token"].as_str().unwrap();

    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&peer_uri)
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"display_name":"Managed peer", "role_ids":[tenant_admin_role]}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("{peer_uri}/reset-password"))
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"new_password":"synthetic-new-peer-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {old_peer_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"peer-admin",
                "password":"synthetic-new-peer-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let login: Value = test::read_body_json(response).await;
    assert_eq!(login["user"]["display_name"], "Managed peer");
    let new_peer_token = login["token"].as_str().unwrap();

    let response = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&peer_uri)
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .set_json(json!({"status":"disabled"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/me")
            .insert_header(("Authorization", format!("Bearer {new_peer_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&peer_uri)
            .insert_header(("Authorization", format!("Bearer {}", fixture.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(json!({"tenant_slug":"default", "username":"peer-admin",
                "password":"synthetic-new-peer-password"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
