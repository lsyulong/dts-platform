// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use std::sync::Arc;

use actix_web::{App, test, web};
use dts_backend::repo::CreateTaskRunParams;
use dts_backend::types::{
    CreateDatasourceParams, CreateTaskParams, CreateTenantParams, CreateUserParams,
};
use dts_server::{
    api::{auth, task_logs},
    models::auth::LoginRequest,
    services::{
        auth_svc::AuthService,
        scheduler::TaskScheduler,
        task_log_svc::{RunLogScope, TaskLogService},
    },
    ws::task_log::WsHub,
};
use serde_json::{Value, json};

async fn reader(ctx: &common::TestBackend) -> String {
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
            email: "reader@test".into(),
            password_hash: AuthService::hash_password_sync("old-password-123").unwrap(),
            display_name: Some("Reader".into()),
        })
        .await
        .unwrap();
    tenant.id
}

async fn pipeline(ctx: &common::TestBackend, tenant: &str, id: &str) {
    let source = format!("source-{id}");
    let target = format!("target-{id}");
    for (datasource, kind, port) in [(&source, "mysql", 3306), (&target, "postgresql", 5432)] {
        ctx.datasource_repo
            .create(&CreateDatasourceParams {
                id: datasource.clone(),
                tenant_id: tenant.into(),
                name: datasource.clone(),
                db_type: kind.into(),
                host: "localhost".into(),
                port,
                username: Some("fixture".into()),
                encrypted_password: None,
                database_name: Some("fixture".into()),
                created_by: "fixture".into(),
            })
            .await
            .unwrap();
    }
    ctx.task_repo.create(&CreateTaskParams { id: id.into(), tenant_id: tenant.into(), name: id.into(), description: None, source_id: source, target_id: target, cron_expr: None, config_json: r#"{"table_mappings":[{"source_table":"orders","target_table":"orders"}],"sync_mode":"full","batch_size":500}"#.into(), created_by: "fixture".into() }).await.unwrap();
}

#[actix_web::test]
async fn profile_is_a_personal_operation_and_rejects_identity_fields() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = reader(&ctx).await;
    let token = common::test_token(&ctx, "reader").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .route("/auth/profile", web::patch().to(auth::update_profile)),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/auth/profile")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"email":"  updated@test  ","display_name":"  本人名称  "}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let user: Value = test::read_body_json(response).await;
    assert_eq!(user["id"], "reader");
    assert_eq!(user["tenant_id"], tenant);
    assert_eq!(user["email"], "updated@test");
    assert_eq!(user["display_name"], "本人名称");
    assert_eq!(user["roles"], json!([]));
    assert_eq!(user["permissions"], json!([]));
    assert!(user.get("password_hash").is_none());
    let response = test::call_service(&app, test::TestRequest::patch().uri("/auth/profile").insert_header(("Authorization", format!("Bearer {token}"))).set_json(json!({"email":"stolen@test","display_name":"Stolen","tenant_id":"foreign","id":"admin"})).to_request()).await;
    assert_eq!(response.status(), 400);
    let actual = ctx
        .user_repo
        .find_detail_by_id("reader")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual.email, "updated@test");
    assert_eq!(actual.tenant_id, tenant);
}

#[actix_web::test]
async fn invalid_current_password_preserves_sessions_success_revokes_only_the_changed_user() {
    let ctx = common::setup_test_backend().await.unwrap();
    reader(&ctx).await;
    let token = common::test_token(&ctx, "reader").await;
    let second = common::test_token(&ctx, "reader").await;
    let other = common::admin_token(&ctx).await;
    let app = test::init_service(App::new().app_data(web::Data::new(ctx.auth.clone())).route(
        "/auth/change-password",
        web::post().to(auth::change_password),
    ))
    .await;
    for (current, new_password) in [("wrong", "new-password-456"), ("old-password-123", "short")] {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/auth/change-password")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .set_json(json!({"current_password":current,"new_password":new_password}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 400);
        assert!(ctx.auth.authenticate(&token).await.is_ok());
        assert!(ctx.auth.authenticate(&second).await.is_ok());
    }
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/change-password")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(
                json!({"current_password":"old-password-123","new_password":"new-password-456"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 204);
    assert!(test::read_body(response).await.is_empty());
    assert!(ctx.auth.authenticate(&token).await.is_err());
    assert!(ctx.auth.authenticate(&second).await.is_err());
    assert!(ctx.auth.authenticate(&other).await.is_ok());
    assert!(
        ctx.auth
            .manager
            .get_all_tokens_by_login_id(dts_server::services::sa_auth::LOGIN_TYPE, "reader")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ctx.auth
            .login(&LoginRequest {
                username: "reader".into(),
                password: "old-password-123".into(),
                tenant_slug: "default".into()
            })
            .await
            .is_err()
    );
    let fresh = ctx
        .auth
        .login(&LoginRequest {
            username: "reader".into(),
            password: "new-password-456".into(),
            tenant_slug: "default".into(),
        })
        .await
        .unwrap();
    assert!(ctx.auth.authenticate(&fresh.token).await.is_ok());
}

#[actix_web::test]
async fn log_http_pages_return_persisted_ws_identity_and_enforce_ownership_and_permission() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = reader(&ctx).await;
    pipeline(&ctx, &tenant, "own-task").await;
    ctx.tenant_repo
        .create(&CreateTenantParams {
            id: "foreign".into(),
            name: "Foreign".into(),
            slug: "foreign".into(),
        })
        .await
        .unwrap();
    pipeline(&ctx, "foreign", "foreign-task").await;
    for (run, task) in [
        ("own-run", "own-task"),
        ("empty-run", "own-task"),
        ("foreign-run", "foreign-task"),
    ] {
        ctx.task_run_repo
            .create(&CreateTaskRunParams {
                id: run.into(),
                task_id: task.into(),
                status: "running".into(),
            })
            .await
            .unwrap();
    }
    let service = TaskLogService::new(ctx.task_log_repo.clone());
    let hub = WsHub::with_log_service(service.clone());
    let mut receiver = hub.tx.subscribe();
    let scope = RunLogScope {
        tenant_id: &tenant,
        task_id: "own-task",
        run_id: "own-run",
    };
    hub.push_run_log(scope, "INFO", "scheduler", "observed start")
        .await
        .unwrap();
    let emitted: Value = serde_json::from_str(&receiver.recv().await.unwrap().1).unwrap();
    hub.push_run_log(scope, "WARN", "scheduler", "observed warning")
        .await
        .unwrap();
    let token = common::admin_token(&ctx).await;
    let reader_token = common::test_token(&ctx, "reader").await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(ctx.auth.clone()))
            .app_data(web::Data::new(service))
            .route(
                "/tasks/{task_id}/runs/{run_id}/logs",
                web::get().to(task_logs::list),
            ),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/tasks/own-task/runs/own-run/logs?offset=0&limit=1")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page["total"], 2);
    assert_eq!(page["data"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"][0]["id"], emitted["id"]);
    assert_eq!(page["data"][0]["sequence"], emitted["sequence"]);
    for (uri, expected) in [
        ("/tasks/own-task/runs/foreign-run/logs", 404),
        ("/tasks/foreign-task/runs/foreign-run/logs", 404),
        ("/tasks/own-task/runs/missing/logs", 404),
        ("/tasks/own-task/runs/own-run/logs?limit=201", 400),
        ("/tasks/own-task/runs/own-run/logs?offset=-1", 400),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(uri)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), expected, "{uri}");
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/tasks/own-task/runs/empty-run/logs")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page, json!({"data":[],"total":0}));
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/tasks/own-task/runs/own-run/logs")
            .insert_header(("Authorization", format!("Bearer {reader_token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
}

struct ObservedEngine;

#[async_trait::async_trait]
impl dts_engine::EngineService for ObservedEngine {
    fn supports(&self, _: &dts_engine::TaskSubmission) -> bool {
        true
    }

    async fn submit(
        &self,
        task: dts_engine::TaskSubmission,
        tx: tokio::sync::mpsc::UnboundedSender<dts_engine::SyncProgress>,
    ) -> Result<String, dts_engine::EngineError> {
        tx.send(dts_engine::SyncProgress {
            run_id: task.run_id.clone(),
            task_id: task.task_id,
            status: dts_engine::RunStatus::Failed,
            rows_synced: 0,
            bytes_synced: 0,
            metrics_available: false,
            total_rows: None,
            current_table: None,
            percent: 0.0,
            message: Some("postgres://fixture:secret@localhost/db".into()),
        })
        .map_err(|_| dts_engine::EngineError::Internal("test progress closed".into()))?;
        Ok(task.run_id)
    }
    async fn cancel(&self, _run_id: &str) -> Result<(), dts_engine::EngineError> {
        Ok(())
    }
}

#[actix_web::test]
async fn scheduler_persists_only_observed_events_and_protects_engine_diagnostics() {
    let ctx = common::setup_test_backend().await.unwrap();
    let tenant = reader(&ctx).await;
    pipeline(&ctx, &tenant, "observed-task").await;
    let service = TaskLogService::new(ctx.task_log_repo.clone());
    let scheduler = TaskScheduler::new(
        Arc::new(ObservedEngine),
        ctx.task_repo.clone(),
        ctx.task_run_repo.clone(),
        ctx.datasource_repo.clone(),
        Arc::new(WsHub::with_log_service(service.clone())),
    );
    let run = scheduler
        .start_task("observed-task", &tenant)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while scheduler.has_active_runs() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let page = service
        .list(
            RunLogScope {
                tenant_id: &tenant,
                task_id: "observed-task",
                run_id: &run,
            },
            0,
            200,
        )
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        page.data.iter().map(|log| log.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(page.data[0].message, "运行实例已建立");
    assert_eq!(page.data[1].message, "引擎已接受任务提交");
    assert_eq!(page.data[2].level, "ERROR");
    assert!(page.data.iter().all(|log| !log.message.contains("secret")));
    let runs = ctx
        .task_run_repo
        .find_recent("observed-task", 1)
        .await
        .unwrap();
    assert_eq!(runs[0].status, "failed");
    assert!(!runs[0].error_message.as_deref().unwrap().contains("secret"));
}
