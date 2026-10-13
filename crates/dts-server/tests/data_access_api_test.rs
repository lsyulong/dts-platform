use std::sync::Arc;

use actix_web::{App, test, web};
use dts_backend::repo::data_access::DataAccessRepo;
use dts_backend_sqlite::{
    SqliteMetadataCatalogRepo, SqliteProductResourceRepo, SqliteRoleRepo, SqliteTenantRepo,
    SqliteUserRepo,
};
use dts_server::{
    api::{
        data_access, product, product_references,
        query::{self, QueryAccess},
    },
    config::SwanLakeConfig,
    services::{
        auth_svc::AuthService,
        data_access::{DataAccessService, QueryAuthorizationService},
        data_masking::{MaskingService, PrivacyKey},
        product::ProductService,
        query_svc::QueryService,
        sa_auth,
    },
};
use serde_json::{Value, json};

#[allow(dead_code)]
#[path = "../../dts-backend-sqlite/tests/data_access_support/mod.rs"]
mod fixtures;

#[actix_web::test]
async fn real_http_request_approval_grant_and_revocation_share_the_same_durable_source() {
    let (pool, access, _) = fixtures::setup().await;
    for (actor, permissions) in [
        (
            "a-requester",
            vec!["data_security:read", "data_security:write", "task:execute"],
        ),
        ("a-approver", vec!["data_security:read"]),
        ("a-other", vec!["data_security:read", "data_security:write"]),
    ] {
        for permission in permissions {
            fixtures::permission(
                &pool,
                actor,
                &format!("{actor}-{}", permission.replace(':', "_")),
                permission,
                false,
            )
            .await;
        }
    }
    let roles = Arc::new(SqliteRoleRepo::new(pool.clone()));
    let auth = AuthService::new(
        Arc::new(SqliteTenantRepo::new(pool.clone())),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        roles.clone(),
        sa_auth::test_manager(roles),
    );
    let requester = auth
        .issue_session_for_user("a-requester")
        .await
        .expect("requester session");
    let approver = auth
        .issue_session_for_user("a-approver")
        .await
        .expect("approver session");
    let other = auth
        .issue_session_for_user("a-other")
        .await
        .expect("other session");
    let resources = Arc::new(SqliteProductResourceRepo::new(pool.clone()));
    let metadata = Arc::new(SqliteMetadataCatalogRepo::new(pool.clone()));
    let access: Arc<dyn DataAccessRepo> = Arc::new(access);
    let binding = QueryAccess {
        tenant_id: Some("a".into()),
    };
    let service = DataAccessService::new(
        access.clone(),
        resources.clone(),
        metadata.clone(),
        binding.clone(),
    );
    let guard = QueryAuthorizationService::new(access.clone(), metadata.clone(), binding.clone());
    let key_dir = tempfile::tempdir().expect("privacy key directory");
    let masking = MaskingService::new(
        resources.clone(),
        metadata,
        Arc::new(SqliteUserRepo::new(pool.clone())),
        PrivacyKey::load_or_create(key_dir.path().to_str().expect("directory path"))
            .expect("privacy key"),
    );
    let engine = QueryService::new(&SwanLakeConfig::default());
    let product = ProductService::new(resources, engine.clone(), Some("a".into()));
    let users: Arc<dyn dts_backend::repo::UserRepo> = Arc::new(SqliteUserRepo::new(pool.clone()));
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth))
            .app_data(web::Data::new(service))
            .app_data(web::Data::new(guard))
            .app_data(web::Data::new(masking))
            .app_data(web::Data::new(product))
            .app_data(web::Data::new(engine))
            .app_data(web::Data::new(binding))
            .app_data(web::Data::new(users))
            .route(
                "/references/users",
                web::get().to(product_references::users),
            )
            .route("/resources/{kind}", web::post().to(product::create))
            .route("/resources/{kind}/{id}", web::get().to(product::get))
            .route("/approvals", web::get().to(data_access::approvals))
            .route(
                "/requests/{id}/actions/{action}",
                web::post().to(data_access::decision),
            )
            .route("/grants", web::get().to(data_access::grants))
            .route("/audit", web::get().to(data_access::audit))
            .route("/query", web::post().to(query::execute))
            .route("/schemas", web::get().to(query::schemas)),
    )
    .await;
    let directory = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/references/users?resource_kind=data_access_request&limit=1&offset=1")
            .insert_header(("Authorization", format!("Bearer {requester}")))
            .to_request(),
    )
    .await;
    assert_eq!(directory.status(), 200);
    let directory: Value = test::read_body_json(directory).await;
    assert_eq!(directory["total"], 3);
    assert_eq!(directory["data"].as_array().expect("user page").len(), 1);
    for entry in directory["data"].as_array().expect("user page") {
        assert_eq!(entry["tenant_id"], "a");
        assert!(
            entry.as_object().expect("user option").keys().all(|key| [
                "id",
                "tenant_id",
                "username",
                "display_name"
            ]
            .contains(&key.as_str()))
        );
    }
    for (uri, status) in [
        ("/references/users?resource_kind=masking_rule", 403),
        (
            "/references/users?resource_kind=data_access_request&tenant_id=b",
            400,
        ),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(uri)
                .insert_header(("Authorization", format!("Bearer {requester}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), status);
    }
    let literal = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/references/users?resource_kind=data_access_request&q=%25")
            .insert_header(("Authorization", format!("Bearer {requester}")))
            .to_request(),
    )
    .await;
    assert_eq!(literal.status(), 200);
    let literal: Value = test::read_body_json(literal).await;
    assert_eq!(literal["total"], 0);
    let input = json!({"code":"orders_read","name":"Orders read","description":null,"parent_id":null,
        "properties":{"object_id":"metadata-a","columns":["id"],"purpose":"Daily orders analysis"}});
    let create = test::TestRequest::post()
        .uri("/resources/data_access_request")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .set_json(&input)
        .to_request();
    let response = test::call_service(&app, create).await;
    assert_eq!(response.status(), 201);
    let draft: Value = test::read_body_json(response).await;
    let id = draft["id"].as_str().expect("request id");
    assert_eq!(draft["created_by"], "a-requester");
    let unseen = test::TestRequest::get()
        .uri(&format!("/resources/data_access_request/{id}"))
        .insert_header(("Authorization", format!("Bearer {other}")))
        .to_request();
    assert_eq!(test::call_service(&app, unseen).await.status(), 404);
    let submit = test::TestRequest::post().uri(&format!("/requests/{id}/actions/submit")).insert_header(("Authorization", format!("Bearer {requester}"))).set_json(json!({"expected_version":draft["version"],"payload":{"idempotency_key":"http-submit"}})).to_request();
    let response = test::call_service(&app, submit).await;
    assert_eq!(response.status(), 200);
    let pending: Value = test::read_body_json(response).await;
    assert_eq!(pending["resource"]["status"], "pending");
    let deny_approvals = test::TestRequest::get()
        .uri("/approvals")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .to_request();
    assert_eq!(test::call_service(&app, deny_approvals).await.status(), 403);
    let approvals = test::TestRequest::get()
        .uri("/approvals?status=pending")
        .insert_header(("Authorization", format!("Bearer {approver}")))
        .to_request();
    let rows: Value = test::call_and_read_body_json(&app, approvals).await;
    assert_eq!(rows["data"][0]["id"], id);
    assert_eq!(rows["data"][0]["kind"], "data_access_approval");
    let approve_body = json!({"expected_version":pending["resource"]["version"],"payload":{"idempotency_key":"http-approve","reason":"Approved for daily analysis"}});
    let approve = test::TestRequest::post()
        .uri(&format!("/requests/{id}/actions/approve"))
        .insert_header(("Authorization", format!("Bearer {approver}")))
        .set_json(&approve_body)
        .to_request();
    let response = test::call_service(&app, approve).await;
    assert_eq!(response.status(), 200);
    let approved: Value = test::read_body_json(response).await;
    assert_eq!(approved["result"]["grant"]["columns"], json!(["id"]));
    let replay = test::TestRequest::post()
        .uri(&format!("/requests/{id}/actions/approve"))
        .insert_header(("Authorization", format!("Bearer {approver}")))
        .set_json(&approve_body)
        .to_request();
    let repeated: Value = test::call_and_read_body_json(&app, replay).await;
    assert_eq!(
        repeated["result"]["decision_id"],
        approved["result"]["decision_id"]
    );
    assert_eq!(repeated["result"]["replayed"], true);
    let grants = test::TestRequest::get()
        .uri("/grants")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .to_request();
    let granted: Value = test::call_and_read_body_json(&app, grants).await;
    assert_eq!(granted["data"][0]["table_name"], "orders");
    assert_eq!(granted["data"][0]["columns"], json!(["id"]));
    assert!(
        granted["data"][0].get("target").is_none(),
        "Unrequested column schema stays private"
    );
    let schemas = test::TestRequest::get()
        .uri("/schemas")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .to_request();
    let visible: Value = test::call_and_read_body_json(&app, schemas).await;
    assert_eq!(visible[0]["catalog"], "warehouse");
    assert_eq!(visible[0]["tables"][0]["name"], "orders");
    let forbidden = test::TestRequest::post()
        .uri("/query")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .set_json(json!({"sql":"SELECT amount FROM warehouse.main.orders"}))
        .to_request();
    assert_eq!(test::call_service(&app, forbidden).await.status(), 403);
    let permitted_without_engine = test::TestRequest::post()
        .uri("/query")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .set_json(json!({"sql":"SELECT id FROM warehouse.main.orders"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, permitted_without_engine)
            .await
            .status(),
        400,
        "Unavailable engine must never return fake rows"
    );
    let revoke = test::TestRequest::post().uri(&format!("/requests/{id}/actions/revoke")).insert_header(("Authorization", format!("Bearer {approver}"))).set_json(json!({"expected_version":approved["resource"]["version"],"payload":{"idempotency_key":"http-revoke","reason":"Analysis finished"}})).to_request();
    let response = test::call_service(&app, revoke).await;
    assert_eq!(response.status(), 200);
    let after_revoke = test::TestRequest::get()
        .uri("/grants")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .to_request();
    let current: Value = test::call_and_read_body_json(&app, after_revoke).await;
    assert_eq!(current["data"], json!([]));
    let blocked = test::TestRequest::post()
        .uri("/query")
        .insert_header(("Authorization", format!("Bearer {requester}")))
        .set_json(json!({"sql":"SELECT id FROM warehouse.main.orders"}))
        .to_request();
    assert_eq!(test::call_service(&app, blocked).await.status(), 403);
    let audit = test::TestRequest::get()
        .uri("/audit")
        .insert_header(("Authorization", format!("Bearer {approver}")))
        .to_request();
    let events: Value = test::call_and_read_body_json(&app, audit).await;
    assert!(
        events["data"]
            .as_array()
            .expect("audit page")
            .iter()
            .any(|event| event["action"] == "query" && event["outcome"] == "denied")
    );
    assert!(
        !events.to_string().contains("SELECT id"),
        "Access audit stores hashes and evidence, not raw SQL"
    );
}
