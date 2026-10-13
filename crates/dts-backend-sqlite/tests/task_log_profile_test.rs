// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    BackendError,
    repo::{TaskLogRepo, UserRepo, task_log::AppendTaskLog},
};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteTaskLogRepo, SqliteUserRepo};
use sqlx::SqlitePool;

async fn setup() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO tenant(id,name,slug) VALUES('a','A','a'),('b','B','b');
        INSERT INTO "user"(id,tenant_id,username,email,password_hash) VALUES('user-a','a','reader','a@test','old-hash');
        INSERT INTO datasource(id,tenant_id,name,db_type,host,port,created_by)
          VALUES('ds-a','a','A','mysql','localhost',3306,'user-a'),('ds-b','b','B','mysql','localhost',3306,'user-b');
        INSERT INTO sync_task(id,tenant_id,name,source_id,target_id,created_by)
          VALUES('task-a','a','A','ds-a','ds-a','user-a'),('task-b','b','B','ds-b','ds-b','user-b');
        INSERT INTO task_run(id,task_id) VALUES('run-a','task-a'),('run-a-empty','task-a'),('run-b','task-b');
    "#).execute(&pool).await.unwrap();
    pool
}

fn log(id: &str) -> AppendTaskLog {
    AppendTaskLog {
        id: id.into(),
        tenant_id: "a".into(),
        task_id: "task-a".into(),
        run_id: "run-a".into(),
        timestamp: "2026-10-08T00:00:00Z".into(),
        level: "INFO".into(),
        component: "scheduler".into(),
        message: "observed event".into(),
    }
}

#[tokio::test]
async fn log_sequences_and_total_survive_new_repository_instances() {
    let pool = setup().await;
    let repo = SqliteTaskLogRepo::new(pool.clone());
    for index in 1..=3 {
        let record = repo.append(&log(&format!("event-{index}"))).await.unwrap();
        assert_eq!(record.sequence, index);
    }
    let reopened = SqliteTaskLogRepo::new(pool);
    let page = reopened.list("a", "task-a", "run-a", 1, 1).await.unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].id, "event-2");
    assert_eq!(page.data[0].sequence, 2);
    assert!(
        reopened
            .list("a", "task-a", "run-a", 9, 1)
            .await
            .unwrap()
            .data
            .is_empty()
    );
}

#[tokio::test]
async fn logs_require_the_complete_tenant_task_run_identity() {
    let pool = setup().await;
    let repo = SqliteTaskLogRepo::new(pool.clone());
    repo.append(&log("actual")).await.unwrap();
    for (tenant, task, run) in [
        ("b", "task-a", "run-a"),
        ("a", "task-b", "run-a"),
        ("a", "task-a", "run-b"),
        ("a", "task-a", "missing"),
    ] {
        assert!(matches!(
            repo.list(tenant, task, run, 0, 20).await,
            Err(BackendError::NotFound(_))
        ));
        let mut invalid = log("unowned");
        invalid.tenant_id = tenant.into();
        invalid.task_id = task.into();
        invalid.run_id = run.into();
        assert!(matches!(
            repo.append(&invalid).await,
            Err(BackendError::NotFound(_))
        ));
    }
    assert_eq!(
        repo.list("a", "task-a", "run-a-empty", 0, 20)
            .await
            .unwrap()
            .total,
        0
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_run_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn database_constraints_prevent_mismatched_identity_and_oversized_utf8() {
    let pool = setup().await;
    let mismatch = sqlx::query("INSERT INTO task_run_log(id,tenant_id,task_id,run_id,sequence,timestamp,level,component,message) VALUES('bad','b','task-b','run-a',1,'now','INFO','scheduler','message')")
        .execute(&pool).await;
    assert!(mismatch.is_err());
    let repo = SqliteTaskLogRepo::new(pool.clone());
    let mut oversized = log("oversized");
    oversized.message = "中".repeat(3000);
    assert!(repo.append(&oversized).await.is_err());
    repo.append(&log("kept")).await.unwrap();
    sqlx::query("DELETE FROM task_run WHERE id='run-a'")
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_run_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn rejects_unbounded_log_pagination() {
    let repo = SqliteTaskLogRepo::new(setup().await);
    for (offset, limit) in [(-1, 20), (0, 0), (0, 201)] {
        assert!(matches!(
            repo.list("a", "task-a", "run-a", offset, limit).await,
            Err(BackendError::ConstraintViolation(_))
        ));
    }
}

#[tokio::test]
async fn self_profile_updates_never_cross_tenants_or_change_credentials() {
    let repo = SqliteUserRepo::new(setup().await);
    assert!(
        !repo
            .update_self_profile("user-a", "b", "stolen@test", "Stolen")
            .await
            .unwrap()
    );
    assert!(
        repo.update_self_profile("user-a", "a", "new@test", "New Name")
            .await
            .unwrap()
    );
    let user = repo
        .find_by_tenant_and_username("a", "reader")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.email, "new@test");
    assert_eq!(user.display_name.as_deref(), Some("New Name"));
    assert_eq!(user.password_hash, "old-hash");
    repo.update(
        "user-a",
        &dts_backend::UpdateUserParams {
            display_name: None,
            status: Some("disabled".into()),
        },
    )
    .await
    .unwrap();
    assert!(
        !repo
            .update_self_profile("user-a", "a", "later@test", "Later")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn password_compare_exchange_allows_one_winner_and_protects_tenant_and_status() {
    let repo = SqliteUserRepo::new(setup().await);
    assert!(
        !repo
            .compare_exchange_password("user-a", "b", "old-hash", "stolen")
            .await
            .unwrap()
    );
    let (first, second) = tokio::join!(
        repo.compare_exchange_password("user-a", "a", "old-hash", "hash-1"),
        repo.compare_exchange_password("user-a", "a", "old-hash", "hash-2"),
    );
    assert_ne!(first.unwrap(), second.unwrap());
    let user = repo
        .find_by_tenant_and_username("a", "reader")
        .await
        .unwrap()
        .unwrap();
    assert!(["hash-1", "hash-2"].contains(&user.password_hash.as_str()));
    repo.update(
        "user-a",
        &dts_backend::UpdateUserParams {
            display_name: None,
            status: Some("disabled".into()),
        },
    )
    .await
    .unwrap();
    assert!(
        !repo
            .compare_exchange_password("user-a", "a", &user.password_hash, "late-hash")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn inactive_tenant_blocks_self_updates_in_the_write_statement() {
    let pool = setup().await;
    let repo = SqliteUserRepo::new(pool.clone());
    sqlx::query("UPDATE tenant SET status='disabled' WHERE id='a'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !repo
            .update_self_profile("user-a", "a", "later@test", "Later")
            .await
            .unwrap()
    );
    assert!(
        !repo
            .compare_exchange_password("user-a", "a", "old-hash", "later-hash")
            .await
            .unwrap()
    );
}
