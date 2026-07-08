use dts_backend::repo::TenantRepo;
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteTenantRepo};
use dts_core::types::CreateTenantParams;
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteTenantRepo) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");
    let repo = SqliteTenantRepo::new(pool.clone());
    (pool, repo)
}

#[tokio::test]
async fn create_and_find_tenant() {
    let (_pool, repo) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();
    let params = CreateTenantParams {
        id: id.clone(),
        name: "Test Tenant".into(),
        slug: "test-create".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let found = repo
        .find_by_slug("test-create")
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(found.name, "Test Tenant");
    assert_eq!(found.status, "active");
}

#[tokio::test]
async fn find_by_id_returns_none_for_missing() {
    let (_pool, repo) = setup().await;
    let result = repo.find_by_id("nonexistent").await.expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn list_all_returns_created_tenants() {
    let (_pool, repo) = setup().await;
    repo.create(&CreateTenantParams {
        id: uuid::Uuid::new_v4().to_string(),
        name: "A".into(),
        slug: "a".into(),
    })
    .await
    .expect("create A failed");
    repo.create(&CreateTenantParams {
        id: uuid::Uuid::new_v4().to_string(),
        name: "B".into(),
        slug: "b".into(),
    })
    .await
    .expect("create B failed");

    let all = repo.list_all().await.expect("list failed");
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn count_returns_correct_number() {
    let (_pool, repo) = setup().await;
    assert_eq!(repo.count().await.expect("count failed"), 0);

    repo.create(&CreateTenantParams {
        id: uuid::Uuid::new_v4().to_string(),
        name: "X".into(),
        slug: "x".into(),
    })
    .await
    .expect("create failed");
    assert_eq!(repo.count().await.expect("count failed"), 1);
}
