// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{
    BackendError,
    repo::{product_resource::*, tag_assignment::*},
};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{
    SqliteMigrationRunner, SqliteProductResourceRepo, SqliteTagAssignmentRepo,
};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

async fn setup() -> (
    SqlitePool,
    SqliteProductResourceRepo,
    SqliteTagAssignmentRepo,
) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("memory SQLite");
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .expect("migrations");
    for tenant in ["tenant-a", "tenant-b"] {
        sqlx::query("INSERT INTO tenant(id,name,slug) VALUES(?1,?1,?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("tenant fixture");
        sqlx::query("INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES(?1,?2,?1,?3,'hash')")
            .bind(format!("actor-{tenant}"))
            .bind(tenant)
            .bind(format!("{tenant}@example.test"))
            .execute(&pool)
            .await
            .expect("actor fixture");
    }
    let resources = SqliteProductResourceRepo::new(pool.clone());
    let assignments = SqliteTagAssignmentRepo::new(pool.clone());
    (pool, resources, assignments)
}

fn create(tenant_id: &str, id: &str, kind: &str) -> CreateResourceParams {
    CreateResourceParams {
        id: id.into(),
        tenant_id: tenant_id.into(),
        kind: kind.into(),
        actor_id: format!("actor-{tenant_id}"),
        fields: ResourceWrite {
            code: format!("{kind}-{id}"),
            name: format!("{kind} {id}"),
            description: None,
            parent_id: None,
            status: "enabled".into(),
            properties: json!({}),
            references: Vec::new(),
        },
    }
}

fn command(key: &str, target_id: &str) -> TagAssignmentCommand {
    TagAssignmentCommand {
        tenant_id: "tenant-a".into(),
        actor_id: "actor-tenant-a".into(),
        tag_id: "tag-one".into(),
        expected_tag_version: 1,
        target_kind: "asset_entry".into(),
        target_id: target_id.into(),
        idempotency_key: key.into(),
    }
}

fn filter() -> TagAssignmentFilter {
    TagAssignmentFilter {
        tenant_id: "tenant-a".into(),
        tag_id: "tag-one".into(),
        limit: 20,
        offset: 0,
    }
}

async fn fixtures(resources: &SqliteProductResourceRepo) {
    resources
        .create(&create("tenant-a", "tag-one", "tag"))
        .await
        .expect("tag");
    for target in ["asset-one", "asset-two"] {
        resources
            .create(&create("tenant-a", target, "asset_entry"))
            .await
            .expect("target");
    }
    resources
        .create(&create("tenant-b", "other-asset", "asset_entry"))
        .await
        .expect("other tenant target");
}

#[tokio::test]
async fn assignment_round_trip_is_idempotent_and_receipts_are_immutable() {
    let (pool, resources, assignments) = setup().await;
    fixtures(&resources).await;
    let assign = command("assign-one", "asset-one");
    let created = assignments.assign(&assign).await.expect("assign");
    assert!(created.active);
    assert!(!created.replayed);
    let replay = assignments.assign(&assign).await.expect("replay");
    assert!(replay.replayed);
    assert_eq!(replay.assignment.id, created.assignment.id);
    let already_active = assignments
        .assign(&command("assign-two", "asset-one"))
        .await
        .expect("already active");
    assert!(!already_active.replayed);
    assert_eq!(already_active.assignment.id, created.assignment.id);
    let page = assignments.list(&filter()).await.expect("list");
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].target_code, "asset_entry-asset-one");

    let removed = assignments
        .unassign(&command("unassign-one", "asset-one"))
        .await
        .expect("unassign");
    assert!(!removed.active);
    assert_eq!(assignments.list(&filter()).await.unwrap().total, 0);
    assert!(
        sqlx::query("UPDATE tag_assignment_receipt SET active=1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM tag_assignment_receipt")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn active_assignments_protect_both_resources_until_unassigned() {
    let (_pool, resources, assignments) = setup().await;
    fixtures(&resources).await;
    assignments
        .assign(&command("assign-protected", "asset-one"))
        .await
        .unwrap();
    assert!(
        resources
            .delete("tenant-a", "tag", "tag-one", 1, "actor-tenant-a")
            .await
            .is_err()
    );
    assert!(
        resources
            .delete("tenant-a", "asset_entry", "asset-one", 1, "actor-tenant-a")
            .await
            .is_err()
    );
    assignments
        .unassign(&command("unassign-protected", "asset-one"))
        .await
        .unwrap();
    resources
        .delete("tenant-a", "asset_entry", "asset-one", 1, "actor-tenant-a")
        .await
        .expect("delete after unassign");
}

#[tokio::test]
async fn stale_versions_cross_tenant_targets_and_reused_keys_are_rejected() {
    let (_pool, resources, assignments) = setup().await;
    fixtures(&resources).await;
    let first = command("same-key", "asset-one");
    assignments.assign(&first).await.unwrap();
    assert!(matches!(
        assignments.assign(&command("same-key", "asset-two")).await,
        Err(BackendError::Conflict(_))
    ));
    assert!(matches!(
        assignments.unassign(&first).await,
        Err(BackendError::Conflict(_))
    ));
    let mut stale = command("stale", "asset-two");
    stale.expected_tag_version = 2;
    assert!(matches!(
        assignments.assign(&stale).await,
        Err(BackendError::Conflict(_))
    ));
    let mut foreign = command("foreign", "other-asset");
    foreign.target_id = "other-asset".into();
    assert!(matches!(
        assignments.assign(&foreign).await,
        Err(BackendError::NotFound(_))
    ));
}

#[tokio::test]
async fn audit_failure_rolls_back_the_assignment_and_receipt() {
    let (pool, resources, assignments) = setup().await;
    fixtures(&resources).await;
    sqlx::raw_sql(
        "CREATE TRIGGER reject_tag_audit BEFORE INSERT ON product_resource_audit \
         WHEN NEW.resource_kind='tag' AND NEW.operation='update' \
         BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        assignments
            .assign(&command("rollback", "asset-one"))
            .await
            .is_err()
    );
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_assignment")
        .fetch_one(&pool)
        .await
        .unwrap();
    let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag_assignment_receipt")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((active, receipts), (0, 0));
}
