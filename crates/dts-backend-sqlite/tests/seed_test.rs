use dts_backend::repo::SeedRepo;
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteSeedRepo};
use sqlx::SqlitePool;

#[tokio::test]
async fn has_data_returns_false_on_empty_database() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let repo = SqliteSeedRepo::new(pool);
    let has_data = repo.has_data().await.expect("has_data failed");
    assert!(!has_data, "fresh database should have no data");
}

#[tokio::test]
async fn seed_populates_default_data_and_has_data_returns_true() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let repo = SqliteSeedRepo::new(pool.clone());

    // Verify empty before seed
    assert!(!repo.has_data().await.expect("has_data failed"));

    // Run seed
    repo.seed().await.expect("seed failed");

    // Verify has data after seed
    assert!(repo.has_data().await.expect("has_data after seed failed"));

    // Verify concrete data: tenant exists
    let tenant_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tenant WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("count tenant failed");
    assert_eq!(tenant_count, 1);

    // Verify role exists
    let role_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM role")
        .fetch_one(&pool)
        .await
        .expect("count role failed");
    assert_eq!(role_count, 3); // admin, developer, viewer

    // Verify user exists
    let user_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM \"user\" WHERE username = 'admin'")
            .fetch_one(&pool)
            .await
        .expect("count user failed");
    assert_eq!(user_count, 1);

    // Verify datasources exist
    let ds_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM datasource")
        .fetch_one(&pool)
        .await
        .expect("count datasource failed");
    assert_eq!(ds_count, 2);
}

#[tokio::test]
async fn seed_fails_on_second_call_due_to_unique_slug() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let repo = SqliteSeedRepo::new(pool);

    repo.seed().await.expect("first seed failed");

    // Second seed should fail because tenant.slug='default' already exists.
    // The proper pattern is: call has_data() first, then seed() only if false.
    let result = repo.seed().await;
    assert!(result.is_err(), "second seed should fail with UNIQUE constraint");
}
