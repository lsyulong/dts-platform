//! 数据库层集成测试

mod common;

use dts_server::db;
use sqlx::Row;

#[actix_web::test]
async fn create_pool_with_memory_db_works() {
    let pool = db::create_pool("sqlite::memory:").await;
    assert!(pool.is_ok());
    let pool = pool.unwrap();

    let result: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(result, 1);
}

#[actix_web::test]
async fn run_migrations_creates_all_tables() {
    let pool = common::setup_test_db().await.unwrap();

    let tables = vec![
        "tenant", "user", "role", "user_role", "role_permission",
        "datasource", "sync_task", "task_run", "alert_rule", "audit_log",
    ];

    for table in &tables {
        let count: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'", table)
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count > 0, "Table '{}' should exist after migration", table);
    }
}

#[actix_web::test]
async fn run_migrations_is_idempotent() {
    let pool = common::setup_test_db().await.unwrap();
    let result = db::run_migrations(&pool).await;
    assert!(result.is_ok());
}

#[actix_web::test]
async fn seed_default_data_creates_initial_data() {
    let pool = common::setup_test_db().await.unwrap();

    db::seed_default_data(&pool).await.unwrap();

    let tenant_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tenant_count, 1);

    let role_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM role")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role_count, 3, "Should create 3 default roles");

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(user_count, 1);

    let ds_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM datasource")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ds_count, 2, "Should create 2 demo datasources");
}

#[actix_web::test]
async fn seed_default_data_is_idempotent() {
    let pool = common::setup_test_db().await.unwrap();

    db::seed_default_data(&pool).await.unwrap();
    db::seed_default_data(&pool).await.unwrap();

    let tenant_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tenant_count, 1, "Idempotent seed should not create duplicate tenants");
}

#[actix_web::test]
async fn foreign_key_constraints_are_enforced() {
    let pool = common::setup_test_db().await.unwrap();

    let result = sqlx::query(
        "INSERT INTO user (id, tenant_id, username, email, password_hash) VALUES (?, ?, ?, ?, ?)"
    )
    .bind("bad-user")
    .bind("non-existent-tenant")
    .bind("test")
    .bind("test@test.com")
    .bind("hash")
    .execute(&pool)
    .await;

    assert!(result.is_err(), "FK constraint should prevent insert with invalid tenant_id");
}
