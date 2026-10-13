use dts_backend::{BackendError, repo::notification_connection::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteNotificationConnectionRepo};
use serde_json::json;
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteNotificationConnectionRepo) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other')").execute(&pool).await.unwrap();
    (pool.clone(), SqliteNotificationConnectionRepo::new(pool))
}

fn create(tenant: &str) -> CreateNotificationConnectionParams {
    CreateNotificationConnectionParams {
        id: "connection".into(),
        tenant_id: tenant.into(),
        name: "生产告警".into(),
        channel: "webhook".into(),
        public_config: json!({"url":"https://example.test/hook"}),
        secret: Some("encrypted".into()),
        enabled: true,
        actor_id: "actor".into(),
    }
}

#[tokio::test]
async fn list_hides_secret_and_tenant_isolation_is_enforced() {
    let (_pool, repo) = setup().await;
    repo.create(&create("tenant")).await.unwrap();
    let list = repo.list("tenant").await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].secret.is_none());
    assert!(list[0].has_secret);
    assert!(repo.list("other").await.unwrap().is_empty());
    assert!(repo.get("other", "connection").await.unwrap().is_none());
    assert_eq!(
        repo.get("tenant", "connection")
            .await
            .unwrap()
            .unwrap()
            .secret
            .as_deref(),
        Some("encrypted")
    );
}

#[tokio::test]
async fn update_uses_optimistic_version_and_can_preserve_secret() {
    let (_pool, repo) = setup().await;
    repo.create(&create("tenant")).await.unwrap();
    let updated = repo
        .update(&UpdateNotificationConnectionParams {
            tenant_id: "tenant".into(),
            id: "connection".into(),
            expected_version: 1,
            name: "新名称".into(),
            public_config: json!({"url":"https://example.test/new"}),
            replace_secret: false,
            secret: None,
            actor_id: "actor".into(),
        })
        .await
        .unwrap();
    assert_eq!(updated.version, 2);
    assert_eq!(updated.secret.as_deref(), Some("encrypted"));
    let stale = repo
        .set_enabled(&SetNotificationConnectionEnabledParams {
            tenant_id: "tenant".into(),
            id: "connection".into(),
            expected_version: 1,
            enabled: false,
            actor_id: "actor".into(),
        })
        .await;
    assert!(matches!(stale, Err(BackendError::Conflict(_))));
}

#[tokio::test]
async fn delete_is_soft_and_audit_is_immutable() {
    let (pool, repo) = setup().await;
    repo.create(&create("tenant")).await.unwrap();
    repo.delete("tenant", "connection", 1, "actor")
        .await
        .unwrap();
    assert!(repo.get("tenant", "connection").await.unwrap().is_none());
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM notification_connection WHERE id='connection' AND deleted_at IS NOT NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    assert!(
        sqlx::query("DELETE FROM notification_connection_audit")
            .execute(&pool)
            .await
            .is_err()
    );
}
