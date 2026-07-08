use dts_backend::repo::{AlertRepo, TenantRepo};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteAlertRepo, SqliteMigrationRunner, SqliteTenantRepo};
use dts_core::types::{CreateAlertParams, CreateTenantParams};
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteAlertRepo, String) {
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
            slug: format!("alert-test-{}", &tenant_id[..8]),
        })
        .await
        .expect("tenant creation failed");

    let repo = SqliteAlertRepo::new(pool.clone());
    (pool, repo, tenant_id)
}

#[tokio::test]
async fn create_and_list_alert() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    let params = CreateAlertParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        task_id: None,
        name: "High Latency Alert".into(),
        metric_type: "latency".into(),
        operator: ">".into(),
        threshold: 5000.0,
        notify_channels: r#"["email","sms"]"#.into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "High Latency Alert");
    assert_eq!(list[0].metric_type, "latency");
    assert_eq!(list[0].operator, ">");
    assert_eq!(list[0].threshold, 5000.0);
    assert!(list[0].enabled); // default is enabled (1)
    assert_eq!(list[0].notify_channels, r#"["email","sms"]"#);
    assert!(list[0].task_id.is_none());
    assert!(list[0].task_name.is_none());
}

#[tokio::test]
async fn list_by_tenant_returns_empty_for_new_tenant() {
    let (_pool, repo, tenant_id) = setup().await;
    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert!(list.is_empty());
}

#[tokio::test]
async fn toggle_disables_and_enables_alert() {
    let (_pool, repo, tenant_id) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();

    repo.create(&CreateAlertParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        task_id: None,
        name: "Toggle Test".into(),
        metric_type: "error_rate".into(),
        operator: ">=".into(),
        threshold: 0.05,
        notify_channels: "[]".into(),
    })
    .await
    .expect("create failed");

    // Toggle off
    let toggled = repo
        .toggle(&id, &tenant_id)
        .await
        .expect("toggle failed");
    assert!(toggled);

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list after toggle failed");
    assert!(!list[0].enabled);

    // Toggle back on
    repo.toggle(&id, &tenant_id)
        .await
        .expect("toggle back failed");

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list after toggle back failed");
    assert!(list[0].enabled);
}

#[tokio::test]
async fn toggle_returns_false_for_nonexistent() {
    let (_pool, repo, tenant_id) = setup().await;
    let toggled = repo
        .toggle("nonexistent", &tenant_id)
        .await
        .expect("toggle failed");
    assert!(!toggled);
}

#[tokio::test]
async fn list_alert_with_task_shows_task_name() {
    let (pool, repo, tenant_id) = setup().await;

    // Create a datasource and task first
    let ds_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&ds_id)
    .bind(&tenant_id)
    .bind("Task DS")
    .bind("MySQL")
    .bind("localhost")
    .bind(3306_i64)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert datasource failed");

    let task_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sync_task (id, tenant_id, name, source_id, target_id, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&task_id)
    .bind(&tenant_id)
    .bind("My Sync Task")
    .bind(&ds_id)
    .bind(&ds_id)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert task failed");

    // Create alert linked to task
    repo.create(&CreateAlertParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        task_id: Some(task_id),
        name: "Task Alert".into(),
        metric_type: "rows".into(),
        operator: "<".into(),
        threshold: 100.0,
        notify_channels: "[]".into(),
    })
    .await
    .expect("create alert failed");

    let list = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task_name.as_deref(), Some("My Sync Task"));
}
