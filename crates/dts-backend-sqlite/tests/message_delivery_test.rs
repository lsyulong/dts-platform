#![allow(clippy::unwrap_used)]

use dts_backend::{BackendError, repo::message_delivery::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{
    SqliteMigrationRunner, repo::message_delivery::SqliteMessageDeliveryRepo,
};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

async fn setup(recipients: &[&str]) -> (SqlitePool, SqliteMessageDeliveryRepo, DeliverInAppParams) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .unwrap();
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .unwrap();
    for tenant in ["tenant-1", "tenant-2"] {
        sqlx::query("INSERT INTO tenant(id,name,slug) VALUES(?1,?1,?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (id, tenant, status) in [
        ("user-1", "tenant-1", "active"),
        ("user-2", "tenant-1", "active"),
        ("off", "tenant-1", "inactive"),
        ("foreign-user", "tenant-2", "active"),
    ] {
        sqlx::query("INSERT INTO \"user\"(id,tenant_id,username,email,password_hash,status) VALUES(?1,?2,?1,?1,'test_hash',?3)")
            .bind(id).bind(tenant).bind(status).execute(&pool).await.unwrap();
    }
    let properties = json!({"channel":"in_app","recipient_ids":recipients,"content":"source"});
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES('message-1','tenant-1','system_message','notice','Notice','draft',?1,1,'user-1',?2,?2)")
        .bind(properties.to_string()).bind(chrono::Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
    let params = DeliverInAppParams {
        tenant_id: "tenant-1".into(),
        message_id: "message-1".into(),
        expected_version: 1,
        actor_id: "user-1".into(),
        idempotency_key: "request-1".into(),
        subject: "Notice".into(),
        body: "Actual rendered body".into(),
        recipient_ids: recipients.iter().map(|id| (*id).into()).collect(),
    };
    let repo = SqliteMessageDeliveryRepo::new(pool.clone());
    (pool, repo, params)
}

async fn assert_rolled_back(pool: &SqlitePool) {
    for query in [
        "SELECT COUNT(*) FROM message_delivery",
        "SELECT COUNT(*) FROM message_delivery_batch",
        "SELECT COUNT(*) FROM product_resource_audit",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(query)
                .fetch_one(pool)
                .await
                .unwrap(),
            0
        );
    }
    let state: (String, i64) = sqlx::query_as(
        "SELECT status,version FROM product_resource WHERE tenant_id='tenant-1' AND id='message-1'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(state, ("draft".into(), 1));
}

#[tokio::test]
async fn batch_delivery_is_persistent_atomic_and_sanitized() {
    let (pool, repo, params) = setup(&["user-1", "user-2"]).await;
    let batch = repo.deliver_in_app(&params).await.unwrap();
    assert_eq!(batch.data.len(), 2);
    assert_eq!(batch.resource_version, 2);
    assert!(!batch.replayed);
    assert_eq!(batch.data[0].status, "delivered");
    let state: (String, i64) = sqlx::query_as(
        "SELECT status,version FROM product_resource WHERE tenant_id='tenant-1' AND id='message-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("sent".into(), 2));
    let summary: String = sqlx::query_scalar("SELECT summary FROM product_resource_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!summary.contains("Actual rendered body"));
    assert!(!summary.contains("Notice"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&summary).unwrap()["recipient_count"],
        2
    );
    let reopened = SqliteMessageDeliveryRepo::new(pool.clone());
    let inbox = reopened
        .list_inbox(&MessageInboxFilter {
            tenant_id: "tenant-1".into(),
            recipient_id: "user-2".into(),
            unread_only: true,
            limit: 20,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(inbox.total, 1);
    assert_eq!(inbox.data[0].body, "Actual rendered body");
    sqlx::raw_sql(include_str!("../migrations/008_message_deliveries.sql"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM message_delivery")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn exact_idempotent_replay_survives_sent_version_and_recipient_order() {
    let (_, repo, mut params) = setup(&["user-1", "user-2"]).await;
    let first = repo.deliver_in_app(&params).await.unwrap();
    params.recipient_ids.reverse();
    let repeat = repo.deliver_in_app(&params).await.unwrap();
    assert!(repeat.replayed);
    let mut first_ids: Vec<_> = first.data.iter().map(|row| row.id.clone()).collect();
    first_ids.sort();
    let mut repeat_ids: Vec<_> = repeat.data.iter().map(|row| row.id.clone()).collect();
    repeat_ids.sort();
    assert_eq!(first_ids, repeat_ids);
    assert!(
        repo.get_batch("tenant-1", "message-1", 1, "user-1", "request-1")
            .await
            .unwrap()
            .unwrap()
            .replayed
    );
    params.body = "Different body".into();
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert!(matches!(
        repo.get_batch("tenant-1", "message-1", 2, "user-1", "request-1")
            .await,
        Err(BackendError::Conflict(_))
    ));
}

#[tokio::test]
async fn version_duplicate_recipient_and_other_request_cannot_resend() {
    let (pool, repo, mut params) = setup(&["user-1"]).await;
    params.expected_version = 2;
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_rolled_back(&pool).await;
    params.expected_version = 1;
    params.recipient_ids.push("user-1".into());
    assert!(repo.deliver_in_app(&params).await.is_err());
    assert_rolled_back(&pool).await;
    params.recipient_ids.pop();
    repo.deliver_in_app(&params).await.unwrap();
    params.idempotency_key = "request-2".into();
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM message_delivery")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn inactive_foreign_missing_and_replaced_recipients_are_rejected() {
    for recipient in ["off", "foreign-user", "missing-user"] {
        let (pool, repo, params) = setup(&["user-1", recipient]).await;
        assert!(repo.deliver_in_app(&params).await.is_err());
        assert_rolled_back(&pool).await;
    }
    let (pool, repo, mut params) = setup(&["user-1"]).await;
    params.recipient_ids = vec!["user-2".into()];
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_rolled_back(&pool).await;
    params.recipient_ids = vec!["user-1".into()];
    params.actor_id = "foreign-user".into();
    assert!(repo.deliver_in_app(&params).await.is_err());
    assert_rolled_back(&pool).await;
}

#[tokio::test]
async fn cross_tenant_source_and_inbox_reads_are_scoped() {
    let (_, repo, mut params) = setup(&["user-1"]).await;
    let first = repo.deliver_in_app(&params).await.unwrap();
    let id = &first.data[0].id;
    let foreign = repo
        .list_inbox(&MessageInboxFilter {
            tenant_id: "tenant-2".into(),
            recipient_id: "user-1".into(),
            unread_only: false,
            limit: 100,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(foreign.total, 0);
    assert!(matches!(
        repo.mark_read("tenant-2", "user-1", id).await,
        Err(BackendError::NotFound(_))
    ));
    assert!(matches!(
        repo.mark_read("tenant-1", "user-2", id).await,
        Err(BackendError::NotFound(_))
    ));
    assert!(
        repo.get_batch("tenant-2", "message-1", 1, "user-1", "request-1")
            .await
            .unwrap()
            .is_none()
    );
    params.tenant_id = "tenant-2".into();
    params.actor_id = "foreign-user".into();
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::NotFound(_))
    ));
}

#[tokio::test]
async fn mark_read_is_recipient_only_idempotent_and_counts_real_unread() {
    let (pool, repo, params) = setup(&["user-1", "user-2"]).await;
    let first = repo.deliver_in_app(&params).await.unwrap();
    let id = &first
        .data
        .iter()
        .find(|row| row.recipient_id == "user-2")
        .unwrap()
        .id;
    let read = repo.mark_read("tenant-1", "user-2", id).await.unwrap();
    let again = repo.mark_read("tenant-1", "user-2", id).await.unwrap();
    assert_eq!(read.read_at, again.read_at);
    assert_eq!(read.status, "read");
    let unread = repo
        .list_inbox(&MessageInboxFilter {
            tenant_id: "tenant-1".into(),
            recipient_id: "user-2".into(),
            unread_only: true,
            limit: 100,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(unread.total, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_resource_audit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn insertion_failure_rolls_back_previous_recipient_and_batch() {
    let (pool, repo, params) = setup(&["user-1", "user-2"]).await;
    sqlx::raw_sql("CREATE TRIGGER reject_second_delivery BEFORE INSERT ON message_delivery WHEN NEW.recipient_id='user-2' BEGIN SELECT RAISE(ABORT,'test_failure'); END;").execute(&pool).await.unwrap();
    assert!(repo.deliver_in_app(&params).await.is_err());
    assert_rolled_back(&pool).await;
}

#[tokio::test]
async fn audit_failure_rolls_back_deliveries_and_source_status() {
    let (pool, repo, params) = setup(&["user-1"]).await;
    sqlx::raw_sql("CREATE TRIGGER reject_send_audit BEFORE INSERT ON product_resource_audit BEGIN SELECT RAISE(ABORT,'test_failure'); END;").execute(&pool).await.unwrap();
    assert!(repo.deliver_in_app(&params).await.is_err());
    assert_rolled_back(&pool).await;
}

#[tokio::test]
async fn external_channel_does_not_create_fake_delivery_and_history_is_preserved() {
    let (pool, repo, params) = setup(&["user-1"]).await;
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.channel','email') WHERE id='message-1'").execute(&pool).await.unwrap();
    assert!(matches!(
        repo.deliver_in_app(&params).await,
        Err(BackendError::Configuration(_))
    ));
    assert_rolled_back(&pool).await;
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.channel','in_app') WHERE id='message-1'").execute(&pool).await.unwrap();
    repo.deliver_in_app(&params).await.unwrap();
    assert!(
        sqlx::query("DELETE FROM product_resource WHERE tenant_id='tenant-1' AND id='message-1'")
            .execute(&pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE tenant_id='tenant-1' AND id='user-1'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM message_delivery")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
