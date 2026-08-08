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


use dts_backend::repo::{DatasourceRepo, TaskRepo, TenantRepo};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{
    SqliteDatasourceRepo, SqliteMigrationRunner, SqliteTaskRepo, SqliteTenantRepo,
};
use dts_core::types::{CreateDatasourceParams, CreateTaskParams, CreateTenantParams};
use sqlx::SqlitePool;
use uuid::Uuid;

async fn setup() -> (SqlitePool, SqliteTaskRepo, String, String, String) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqliteTenantRepo::new(pool.clone());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Test Tenant".into(),
            slug: format!("task-test-{}", &tenant_id[..8]),
        })
        .await
        .expect("tenant creation failed");

    // Create source and target datasources (required FK)
    let ds_repo = SqliteDatasourceRepo::new(pool.clone());
    let source_id = Uuid::new_v4().to_string();
    let target_id = Uuid::new_v4().to_string();

    ds_repo
        .create(&CreateDatasourceParams {
            id: source_id.clone(),
            tenant_id: tenant_id.clone(),
            name: "Source MySQL".into(),
            db_type: "MySQL".into(),
            host: "src.example.com".into(),
            port: 3306,
            username: None,
            encrypted_password: None,
            database_name: Some("src_db".into()),
            created_by: "user1".into(),
        })
        .await
        .expect("source datasource creation failed");

    ds_repo
        .create(&CreateDatasourceParams {
            id: target_id.clone(),
            tenant_id: tenant_id.clone(),
            name: "Target PG".into(),
            db_type: "PostgreSQL".into(),
            host: "tgt.example.com".into(),
            port: 5432,
            username: None,
            encrypted_password: None,
            database_name: Some("tgt_db".into()),
            created_by: "user1".into(),
        })
        .await
        .expect("target datasource creation failed");

    let repo = SqliteTaskRepo::new(pool.clone());
    (pool, repo, tenant_id, source_id, target_id)
}

#[tokio::test]
async fn create_and_list_task() {
    let (_pool, repo, tenant_id, source_id, target_id) = setup().await;
    let task_id = Uuid::new_v4().to_string();

    let params = CreateTaskParams {
        id: task_id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Sync Task 1".into(),
        description: Some("Test sync task".into()),
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        cron_expr: Some("0 */6 * * *".into()),
        config_json: "{}".into(),
        created_by: "user1".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, task_id);

    let list = repo.list_by_tenant(&tenant_id).await.expect("list failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Sync Task 1");
    assert_eq!(list[0].status, "draft");
    assert_eq!(list[0].source_id, source_id);
    assert_eq!(list[0].target_id, target_id);
}

#[tokio::test]
async fn find_detail_by_id_returns_task_with_datasource_names() {
    let (_pool, repo, tenant_id, source_id, target_id) = setup().await;
    let task_id = Uuid::new_v4().to_string();

    repo.create(&CreateTaskParams {
        id: task_id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Detail Task".into(),
        description: Some("Detail desc".into()),
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        cron_expr: None,
        config_json: r#"{"mode":"full"}"#.into(),
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    let detail = repo
        .find_detail_by_id(&task_id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");

    assert_eq!(detail.name, "Detail Task");
    assert_eq!(detail.source_name, "Source MySQL");
    assert_eq!(detail.source_type, "MySQL");
    assert_eq!(detail.target_name, "Target PG");
    assert_eq!(detail.target_type, "PostgreSQL");
    assert_eq!(detail.status, "draft");
    assert_eq!(detail.config_json, r#"{"mode":"full"}"#);
    assert!(detail.recent_runs.is_empty());
}

#[tokio::test]
async fn find_detail_returns_none_for_missing() {
    let (_pool, repo, tenant_id, _source_id, _target_id) = setup().await;
    let result = repo
        .find_detail_by_id("nonexistent", &tenant_id)
        .await
        .expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn update_status_changes_task_status() {
    let (_pool, repo, tenant_id, source_id, target_id) = setup().await;
    let task_id = Uuid::new_v4().to_string();

    repo.create(&CreateTaskParams {
        id: task_id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Status Task".into(),
        description: None,
        source_id,
        target_id,
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    let updated = repo
        .update_status(&task_id, &tenant_id, "running")
        .await
        .expect("update failed");
    assert!(updated);

    let detail = repo
        .find_detail_by_id(&task_id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.status, "running");
}

#[tokio::test]
async fn update_status_returns_false_for_nonexistent() {
    let (_pool, repo, tenant_id, _source_id, _target_id) = setup().await;
    let updated = repo
        .update_status("nonexistent", &tenant_id, "running")
        .await
        .expect("update failed");
    assert!(!updated);
}

#[tokio::test]
async fn count_by_tenant_and_count_by_status() {
    let (_pool, repo, tenant_id, source_id, target_id) = setup().await;

    assert_eq!(
        repo.count_by_tenant(&tenant_id)
            .await
            .expect("count failed"),
        0
    );

    repo.create(&CreateTaskParams {
        id: Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "Draft Task".into(),
        description: None,
        source_id: source_id.clone(),
        target_id: target_id.clone(),
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    assert_eq!(
        repo.count_by_tenant(&tenant_id)
            .await
            .expect("count failed"),
        1
    );
    assert_eq!(
        repo.count_by_status(&tenant_id, "draft")
            .await
            .expect("count by status failed"),
        1
    );
    assert_eq!(
        repo.count_by_status(&tenant_id, "running")
            .await
            .expect("count by status failed"),
        0
    );
}

#[tokio::test]
async fn find_recent_runs_returns_task_runs() {
    let (pool, repo, tenant_id, source_id, target_id) = setup().await;
    let task_id = Uuid::new_v4().to_string();

    repo.create(&CreateTaskParams {
        id: task_id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Run Task".into(),
        description: None,
        source_id,
        target_id,
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    // Insert some task runs
    let run_id_1 = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO task_run (id, task_id, status, start_time, rows_synced, bytes_synced)
         VALUES (?1, ?2, 'success', datetime('now'), 100, 1024)",
    )
    .bind(&run_id_1)
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("insert run 1 failed");

    let run_id_2 = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO task_run (id, task_id, status, start_time, end_time, rows_synced,
         bytes_synced, error_message)
         VALUES (?1, ?2, 'failed', datetime('now', '-1 hour'), datetime('now'),
         50, 512, 'connection timeout')",
    )
    .bind(&run_id_2)
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("insert run 2 failed");

    let runs = repo
        .find_recent_runs(&task_id, 10)
        .await
        .expect("find runs failed");
    assert_eq!(runs.len(), 2);
    // Most recent first (desc order)
    assert_eq!(runs[0].status, "success");
    assert_eq!(runs[0].rows_synced, 100);
    assert_eq!(runs[1].status, "failed");
    assert_eq!(runs[1].error_message.as_deref(), Some("connection timeout"));
    assert!(runs[1].end_time.is_some());
}
