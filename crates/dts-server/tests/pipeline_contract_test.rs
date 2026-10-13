// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "sqlite")]

mod common;

use std::sync::Arc;

use actix_web::{App, http::StatusCode, test, web};
use async_trait::async_trait;
use dts_backend::types::CreateDatasourceParams;
use dts_engine::{EngineError, EngineService, RunStatus, SyncProgress, TaskSubmission};
use dts_server::{
    api::tasks,
    services::{scheduler::TaskScheduler, task_svc::TaskService},
    ws::task_log::WsHub,
};
use serde_json::{Value, json};

struct ImmediateCompletion;

#[async_trait]
impl EngineService for ImmediateCompletion {
    fn supports(&self, _: &TaskSubmission) -> bool {
        true
    }

    async fn submit(
        &self,
        task: TaskSubmission,
        tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError> {
        tx.send(SyncProgress {
            run_id: task.run_id.clone(),
            task_id: task.task_id,
            status: RunStatus::Completed,
            rows_synced: 42,
            bytes_synced: 128,
            metrics_available: true,
            total_rows: Some(42),
            current_table: None,
            percent: 100.0,
            message: None,
        })
        .map_err(|e| EngineError::Internal(e.to_string()))?;
        Ok(task.run_id)
    }
    async fn cancel(&self, _: &str) -> Result<(), EngineError> {
        Ok(())
    }
}

#[actix_web::test]
async fn typed_pipeline_request_and_immediate_completion_share_the_persisted_run_id() {
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
    for (id, kind) in [
        ("source", "mysql"),
        ("target", "postgresql"),
        ("future-target", "duckdb"),
    ] {
        ctx.datasource_repo
            .create(&CreateDatasourceParams {
                id: id.into(),
                tenant_id: tenant.id.clone(),
                name: id.into(),
                db_type: kind.into(),
                host: "localhost".into(),
                port: if kind == "mysql" { 3306 } else { 5432 },
                username: Some("test".into()),
                encrypted_password: None,
                database_name: Some("test".into()),
                created_by: admin.id.clone(),
            })
            .await
            .unwrap();
    }
    let scheduler = TaskScheduler::new(
        Arc::new(ImmediateCompletion),
        ctx.task_repo.clone(),
        ctx.task_run_repo.clone(),
        ctx.datasource_repo.clone(),
        Arc::new(WsHub::new()),
    );
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(common::test_auth_service(&ctx)))
            .app_data(web::Data::new(TaskService::new(
                ctx.task_repo.clone(),
                ctx.datasource_repo.clone(),
            )))
            .app_data(web::Data::new(scheduler.clone()))
            .app_data(web::Data::new(ctx.task_repo.clone()))
            .route("/tasks", web::post().to(tasks::create))
            .route("/tasks/{id}/start", web::post().to(tasks::start))
            .route("/tasks/{id}/status", web::patch().to(tasks::update_status)),
    )
    .await;
    let token = common::admin_token(&ctx).await;
    let valid = json!({"name":"Orders Pipeline", "source_id":"source", "target_id":"target", "sync_mode":"full",
        "batch_size":500, "table_mappings":[{"source_table":"orders","target_table":"fact_orders","target_schema":"mart"}]});
    for (field, value) in [
        ("table_mappings", json!({"orders":"fact_orders"})),
        ("table_mappings", json!([])),
        ("sync_mode", json!("incremental")),
        ("batch_size", json!(0)),
        ("target_id", json!("future-target")),
        ("cron_expr", json!("0 * * * *")),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        let request = test::TestRequest::post()
            .uri("/tasks")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(&invalid)
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(ctx.task_repo.count_by_tenant(&tenant.id).await.unwrap(), 0);
    let request = test::TestRequest::post()
        .uri("/tasks")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(&valid)
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = test::read_body_json(response).await;
    let task_id = created["id"].as_str().unwrap();
    let stored = ctx
        .task_repo
        .find_detail_by_id(task_id, &tenant.id)
        .await
        .unwrap()
        .unwrap();
    let config: Value = serde_json::from_str(&stored.config_json).unwrap();
    assert_eq!(config["table_mappings"][0]["target_schema"], "mart");
    assert_eq!(config["batch_size"], 500);
    let request = test::TestRequest::post()
        .uri(&format!("/tasks/{task_id}/start"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let started: Value = test::read_body_json(response).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while scheduler.has_active_runs() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let runs = ctx.task_run_repo.find_recent(task_id, 1).await.unwrap();
    assert_eq!(runs[0].id, started["run_id"].as_str().unwrap());
    assert_eq!(runs[0].status, "completed");
    assert_eq!(runs[0].rows_synced, 42);
    assert!(runs[0].end_time.is_some());
    assert!(runs[0].error_message.is_none());
    let request = test::TestRequest::patch()
        .uri(&format!("/tasks/{task_id}/status"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({"status":"running"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, request).await.status(),
        StatusCode::BAD_REQUEST
    );
}
