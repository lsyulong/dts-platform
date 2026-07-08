use dts_backend::repo::MonitoringRepo;
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteMonitoringRepo};
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteMonitoringRepo, String) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let tenant_id = uuid::Uuid::new_v4().to_string();

    // Create tenant
    sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?2, ?3)")
        .bind(&tenant_id)
        .bind("Monitor Tenant")
        .bind("monitor-test")
        .execute(&pool)
        .await
        .expect("insert tenant failed");

    // Create datasources
    let ds1_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, health_status, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'healthy', ?7)",
    )
    .bind(&ds1_id)
    .bind(&tenant_id)
    .bind("Healthy DS")
    .bind("MySQL")
    .bind("localhost")
    .bind(3306_i64)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert healthy ds failed");

    let ds2_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, health_status, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'error', ?7)",
    )
    .bind(&ds2_id)
    .bind(&tenant_id)
    .bind("Error DS")
    .bind("PostgreSQL")
    .bind("bad.example.com")
    .bind(5432_i64)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert error ds failed");

    // Create tasks
    let task1_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sync_task (id, tenant_id, name, source_id, target_id, status, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
    )
    .bind(&task1_id)
    .bind(&tenant_id)
    .bind("Running Task")
    .bind(&ds1_id)
    .bind(&ds2_id)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert running task failed");

    let task2_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sync_task (id, tenant_id, name, source_id, target_id, status, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, 'failed', ?6)",
    )
    .bind(&task2_id)
    .bind(&tenant_id)
    .bind("Failed Task")
    .bind(&ds2_id)
    .bind(&ds1_id)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert failed task failed");

    let task3_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sync_task (id, tenant_id, name, source_id, target_id, status, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6)",
    )
    .bind(&task3_id)
    .bind(&tenant_id)
    .bind("Draft Task")
    .bind(&ds1_id)
    .bind(&ds2_id)
    .bind("user1")
    .execute(&pool)
    .await
    .expect("insert draft task failed");

    // Create task runs (with today's date for synced rows/bytes aggregation)
    sqlx::query(
        "INSERT INTO task_run (id, task_id, status, start_time, rows_synced, bytes_synced)
         VALUES (?1, ?2, 'success', datetime('now'), 500, 2048)",
    )
    .bind(&uuid::Uuid::new_v4().to_string())
    .bind(&task1_id)
    .execute(&pool)
    .await
    .expect("insert run 1 failed");

    sqlx::query(
        "INSERT INTO task_run (id, task_id, status, start_time, rows_synced, bytes_synced)
         VALUES (?1, ?2, 'success', datetime('now'), 300, 1024)",
    )
    .bind(&uuid::Uuid::new_v4().to_string())
    .bind(&task3_id)
    .execute(&pool)
    .await
    .expect("insert run 2 failed");

    let repo = SqliteMonitoringRepo::new(pool.clone());
    (pool, repo, tenant_id)
}

#[tokio::test]
async fn get_overview_returns_correct_counts() {
    let (_pool, repo, tenant_id) = setup().await;

    let overview = repo
        .get_overview(&tenant_id)
        .await
        .expect("get_overview failed");

    assert_eq!(overview.total_tasks, 3);
    assert_eq!(overview.running_tasks, 1);
    assert_eq!(overview.failed_tasks, 1);
    assert_eq!(overview.total_datasources, 2);
    assert_eq!(overview.healthy_datasources, 1);
    assert_eq!(overview.today_synced_rows, 800); // 500 + 300
    assert_eq!(overview.today_synced_bytes, 3072); // 2048 + 1024
    // success_rate = 100 * (3 - 1) / 3 ≈ 66.67
    assert!(overview.success_rate > 66.0 && overview.success_rate < 67.0);
    assert_eq!(overview.recent_runs.len(), 2);
}

#[tokio::test]
async fn get_overview_returns_zeroes_for_empty_tenant() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let empty_tenant_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?2, ?3)")
        .bind(&empty_tenant_id)
        .bind("Empty Tenant")
        .bind("empty")
        .execute(&pool)
        .await
        .expect("insert tenant failed");

    let repo = SqliteMonitoringRepo::new(pool);

    let overview = repo
        .get_overview(&empty_tenant_id)
        .await
        .expect("get_overview failed");

    assert_eq!(overview.total_tasks, 0);
    assert_eq!(overview.running_tasks, 0);
    assert_eq!(overview.failed_tasks, 0);
    assert_eq!(overview.total_datasources, 0);
    assert_eq!(overview.healthy_datasources, 0);
    assert_eq!(overview.today_synced_rows, 0);
    assert_eq!(overview.today_synced_bytes, 0);
    assert_eq!(overview.success_rate, 100.0); // no tasks = 100%
    assert!(overview.recent_runs.is_empty());
}
