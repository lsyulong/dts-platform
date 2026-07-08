use dts_backend::repo::{DatasourceRepo, TenantRepo};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteDatasourceRepo, SqliteMigrationRunner, SqliteTenantRepo};
use dts_core::types::{CreateDatasourceParams, CreateTenantParams};
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteDatasourceRepo, String) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = SqliteTenantRepo::new(pool.clone());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Test Tenant".into(),
            slug: format!("test-{}", &tenant_id[..8]),
        })
        .await
        .expect("tenant creation failed");

    let repo = SqliteDatasourceRepo::new(pool.clone());
    (pool, repo, tenant_id)
}

#[tokio::test]
async fn create_and_list_datasource() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    let params = CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Test MySQL".into(),
        db_type: "MySQL".into(),
        host: "localhost".into(),
        port: 3306,
        username: Some("root".into()),
        encrypted_password: Some("enc:secret".into()),
        database_name: Some("testdb".into()),
        created_by: "user1".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Test MySQL");
    assert_eq!(list[0].db_type, "MySQL");
    assert_eq!(list[0].health_status, "unknown");
}

#[tokio::test]
async fn find_detail_by_id_returns_correct_data() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    repo.create(&CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Detail DS".into(),
        db_type: "PostgreSQL".into(),
        host: "pg.example.com".into(),
        port: 5432,
        username: Some("analyst".into()),
        encrypted_password: Some("enc:pgpass".into()),
        database_name: Some("warehouse".into()),
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    let detail = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.name, "Detail DS");
    assert_eq!(detail.db_type, "PostgreSQL");
    assert_eq!(detail.host, "pg.example.com");
    assert_eq!(detail.port, 5432);
    assert_eq!(detail.username.as_deref(), Some("analyst"));
    assert_eq!(detail.password_masked.as_deref(), Some("******"));
    assert_eq!(detail.health_status, "unknown");
}

#[tokio::test]
async fn find_detail_returns_none_for_missing() {
    let (_pool, repo, tenant_id) = setup().await;
    let result = repo
        .find_detail_by_id("nonexistent", &tenant_id)
        .await
        .expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn delete_removes_and_returns_true() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    repo.create(&CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "To Delete".into(),
        db_type: "SQLite".into(),
        host: "file.db".into(),
        port: 0,
        username: None,
        encrypted_password: None,
        database_name: None,
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    let deleted = repo
        .delete(&id, &tenant_id)
        .await
        .expect("delete failed");
    assert!(deleted);

    let after = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find after delete failed");
    assert!(after.is_none());
}

#[tokio::test]
async fn delete_returns_false_for_nonexistent() {
    let (_pool, repo, tenant_id) = setup().await;
    let deleted = repo
        .delete("nonexistent", &tenant_id)
        .await
        .expect("delete failed");
    assert!(!deleted);
}

#[tokio::test]
async fn exists_returns_true_only_for_existing() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    assert!(!repo
        .exists(&id, &tenant_id)
        .await
        .expect("exists failed"));

    repo.create(&CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Exists Check".into(),
        db_type: "MySQL".into(),
        host: "127.0.0.1".into(),
        port: 3306,
        username: None,
        encrypted_password: None,
        database_name: None,
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    assert!(repo
        .exists(&id, &tenant_id)
        .await
        .expect("exists after create failed"));
}

#[tokio::test]
async fn count_by_tenant_and_count_healthy() {
    let (_pool, repo, tenant_id) = setup().await;

    assert_eq!(
        repo.count_by_tenant(&tenant_id)
            .await
            .expect("count failed"),
        0
    );
    assert_eq!(
        repo.count_healthy_by_tenant(&tenant_id)
            .await
            .expect("count healthy failed"),
        0
    );

    repo.create(&CreateDatasourceParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "Healthy DS".into(),
        db_type: "MySQL".into(),
        host: "healthy.example.com".into(),
        port: 3306,
        username: None,
        encrypted_password: None,
        database_name: None,
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
    // Default health_status is 'unknown', not 'healthy'
    assert_eq!(
        repo.count_healthy_by_tenant(&tenant_id)
            .await
            .expect("count healthy failed"),
        0
    );
}

#[tokio::test]
async fn list_by_tenant_orders_by_created_at_desc() {
    let (_pool, repo, tenant_id) = setup().await;

    // Create first
    repo.create(&CreateDatasourceParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "First".into(),
        db_type: "MySQL".into(),
        host: "host1".into(),
        port: 3306,
        username: None,
        encrypted_password: None,
        database_name: None,
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    // SQLite datetime('now') has second-level granularity.
    // Sleep to ensure different second boundary for created_at.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Create second
    repo.create(&CreateDatasourceParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "Second".into(),
        db_type: "PostgreSQL".into(),
        host: "host2".into(),
        port: 5432,
        username: None,
        encrypted_password: None,
        database_name: None,
        created_by: "user1".into(),
    })
    .await
    .expect("create failed");

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "Second"); // Most recent first
    assert_eq!(list[1].name, "First");
}
