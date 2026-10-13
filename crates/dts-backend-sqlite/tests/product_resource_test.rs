// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_resource::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteProductResourceRepo};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

async fn setup() -> (SqlitePool, SqliteProductResourceRepo) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("memory SQLite");
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .expect("migrations");
    for id in ["tenant-a", "tenant-b"] {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?1, ?1)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("tenant fixture");
    }
    let repo = SqliteProductResourceRepo::new(pool.clone());
    (pool, repo)
}

fn fields(code: &str) -> ResourceWrite {
    ResourceWrite {
        code: code.into(),
        name: format!("配置 {code}"),
        description: Some("配置说明".into()),
        parent_id: None,
        status: "draft".into(),
        properties: json!({"secret": "audit-must-exclude-this", "columns": ["id"]}),
        references: Vec::new(),
    }
}

fn create(id: &str, kind: &str, code: &str) -> CreateResourceParams {
    CreateResourceParams {
        id: id.into(),
        tenant_id: "tenant-a".into(),
        kind: kind.into(),
        actor_id: "actor-a".into(),
        fields: fields(code),
    }
}

fn update(version: i64, code: &str) -> UpdateResourceParams {
    UpdateResourceParams {
        expected_version: version,
        actor_id: "actor-a".into(),
        fields: fields(code),
    }
}

fn filter(tenant: &str, kind: &str) -> ProductResourceFilter {
    ProductResourceFilter {
        tenant_id: tenant.into(),
        kind: kind.into(),
        q: None,
        status: None,
        created_by: None,
        limit: 20,
        offset: 0,
    }
}

#[tokio::test]
async fn resource_roundtrip_update_clear_and_delete_preserves_sanitized_audit() {
    let (_pool, repo) = setup().await;
    let original = repo
        .create(&create("r1", "category", "cat-a"))
        .await
        .expect("create");
    assert_eq!(original.version, 1);
    assert_eq!(original.properties["columns"], json!(["id"]));
    assert_eq!(original.created_by, "actor-a");
    let mut changed = update(1, "cat-b");
    changed.fields.description = None;
    changed.fields.properties = json!({"columns": []});
    changed.fields.status = "enabled".into();
    let saved = repo
        .update("tenant-a", "category", "r1", &changed)
        .await
        .expect("update");
    assert_eq!(saved.version, 2);
    assert_eq!(saved.description, None);
    assert_eq!(saved.status, "enabled");
    assert_eq!(saved.created_at, original.created_at);
    repo.delete("tenant-a", "category", "r1", 2, "actor-b")
        .await
        .expect("delete");
    assert!(
        repo.get("tenant-a", "category", "r1")
            .await
            .expect("get")
            .is_none()
    );
    let audit = repo
        .list_audit("tenant-a", Some("category"), Some("r1"), 20, 0)
        .await
        .expect("audit");
    assert_eq!(audit.total, 3);
    let deleted = audit
        .data
        .iter()
        .find(|entry| entry.operation == "delete")
        .expect("delete audit");
    assert_eq!(deleted.actor_id, "actor-b");
    assert_eq!(deleted.summary["before"]["version"], 2);
    assert_eq!(deleted.summary["before"]["code"], "cat-b");
    assert!(deleted.summary["after"].is_null());
    assert!(
        !serde_json::to_string(&audit)
            .expect("audit JSON")
            .contains("audit-must-exclude-this")
    );
    assert!(
        !serde_json::to_string(&audit)
            .expect("audit JSON")
            .contains("properties")
    );
}

#[tokio::test]
async fn all_resource_operations_and_audit_enforce_tenant_and_kind() {
    let (_pool, repo) = setup().await;
    repo.create(&create("r1", "category", "cat-a"))
        .await
        .expect("create");
    assert!(
        repo.get("tenant-b", "category", "r1")
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        repo.get("tenant-a", "domain", "r1")
            .await
            .expect("get")
            .is_none()
    );
    assert_eq!(
        repo.list(&filter("tenant-b", "category"))
            .await
            .expect("list")
            .total,
        0
    );
    assert_eq!(
        repo.list(&filter("tenant-a", "domain"))
            .await
            .expect("list")
            .total,
        0
    );
    assert!(matches!(
        repo.update("tenant-b", "category", "r1", &update(1, "cat-b"))
            .await,
        Err(BackendError::NotFound(_))
    ));
    assert!(matches!(
        repo.delete("tenant-b", "category", "r1", 1, "actor-b")
            .await,
        Err(BackendError::NotFound(_))
    ));
    assert_eq!(
        repo.list_audit("tenant-b", None, None, 20, 0)
            .await
            .expect("audit")
            .total,
        0
    );
    assert_eq!(
        repo.get("tenant-a", "category", "r1")
            .await
            .expect("get")
            .expect("exists")
            .version,
        1
    );
}

#[tokio::test]
async fn unique_code_and_optimistic_versions_reject_stale_writes_without_audit() {
    let (_pool, repo) = setup().await;
    repo.create(&create("r1", "category", "same"))
        .await
        .expect("create");
    assert!(matches!(
        repo.create(&create("r2", "category", "same")).await,
        Err(BackendError::Conflict(_))
    ));
    let mut other_tenant = create("r2", "category", "same");
    other_tenant.tenant_id = "tenant-b".into();
    repo.create(&other_tenant)
        .await
        .expect("same code in another tenant");
    repo.create(&create("r3", "domain", "same"))
        .await
        .expect("same code in another kind");
    repo.update("tenant-a", "category", "r1", &update(1, "new"))
        .await
        .expect("update");
    assert!(matches!(
        repo.update("tenant-a", "category", "r1", &update(1, "stale"))
            .await,
        Err(BackendError::Conflict(_))
    ));
    assert!(matches!(
        repo.delete("tenant-a", "category", "r1", 1, "actor-a")
            .await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(
        repo.list_audit("tenant-a", Some("category"), Some("r1"), 20, 0)
            .await
            .expect("audit")
            .total,
        2
    );
}

#[tokio::test]
async fn parent_integrity_rejects_cycles_cross_tenant_and_deletion_with_children() {
    let (_pool, repo) = setup().await;
    repo.create(&create("parent", "category", "parent"))
        .await
        .expect("parent");
    let mut child = create("child", "category", "child");
    child.fields.parent_id = Some("parent".into());
    repo.create(&child).await.expect("child");
    let mut cycle = update(1, "parent");
    cycle.fields.parent_id = Some("child".into());
    assert!(matches!(
        repo.update("tenant-a", "category", "parent", &cycle).await,
        Err(BackendError::Conflict(_))
    ));
    assert!(matches!(
        repo.delete("tenant-a", "category", "parent", 1, "actor-a")
            .await,
        Err(BackendError::Conflict(_))
    ));
    let mut outsider = create("outsider", "category", "outside");
    outsider.tenant_id = "tenant-b".into();
    outsider.fields.parent_id = Some("parent".into());
    assert!(matches!(
        repo.create(&outsider).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    let mut wrong_kind = create("wrong", "domain", "wrong");
    wrong_kind.fields.parent_id = Some("parent".into());
    assert!(matches!(
        repo.create(&wrong_kind).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    repo.delete("tenant-a", "category", "child", 1, "actor-a")
        .await
        .expect("delete child");
    repo.delete("tenant-a", "category", "parent", 1, "actor-a")
        .await
        .expect("delete parent");
}

#[tokio::test]
async fn cross_kind_references_roundtrip_and_block_target_deletion_until_unlinked() {
    let (_pool, repo) = setup().await;
    repo.create(&create("domain", "domain", "sales"))
        .await
        .expect("domain");
    let mut process = create("process", "business_process", "orders");
    process.fields.references.push(ResourceReference {
        field: "domain_id".into(),
        target_kind: "domain".into(),
        target_id: "domain".into(),
    });
    let resource = repo.create(&process).await.expect("reference");
    assert_eq!(resource.references, process.fields.references);
    assert_eq!(
        repo.list(&filter("tenant-a", "business_process"))
            .await
            .expect("list")
            .data[0]
            .references,
        process.fields.references
    );
    assert!(matches!(
        repo.delete("tenant-a", "domain", "domain", 1, "actor-a")
            .await,
        Err(BackendError::Conflict(_))
    ));
    let mut foreign = create("foreign", "business_process", "foreign");
    foreign.tenant_id = "tenant-b".into();
    foreign.fields.references = process.fields.references;
    assert!(matches!(
        repo.create(&foreign).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    repo.update(
        "tenant-a",
        "business_process",
        "process",
        &update(1, "orders"),
    )
    .await
    .expect("unlink");
    repo.delete("tenant-a", "domain", "domain", 1, "actor-a")
        .await
        .expect("delete target");
}

#[tokio::test]
async fn audit_failure_rolls_back_resource_and_link_mutations() {
    let (pool, repo) = setup().await;
    repo.create(&create("target", "category", "target"))
        .await
        .expect("target");
    repo.create(&create("source", "category", "source"))
        .await
        .expect("source");
    sqlx::raw_sql(
        "CREATE TRIGGER reject_audit BEFORE INSERT ON product_resource_audit
         BEGIN SELECT RAISE(ABORT, 'synthetic audit failure'); END;",
    )
    .execute(&pool)
    .await
    .expect("failure injection");
    let mut changed = update(1, "changed");
    changed.fields.references.push(ResourceReference {
        field: "related".into(),
        target_kind: "category".into(),
        target_id: "target".into(),
    });
    assert!(
        repo.update("tenant-a", "category", "source", &changed)
            .await
            .is_err()
    );
    let saved = repo
        .get("tenant-a", "category", "source")
        .await
        .expect("get")
        .expect("source");
    assert_eq!(saved.code, "source");
    assert_eq!(saved.version, 1);
    assert!(saved.references.is_empty());
    assert!(
        repo.create(&create("new", "category", "new"))
            .await
            .is_err()
    );
    assert!(
        repo.get("tenant-a", "category", "new")
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        repo.delete("tenant-a", "category", "source", 1, "actor-a")
            .await
            .is_err()
    );
    assert!(
        repo.get("tenant-a", "category", "source")
            .await
            .expect("get")
            .is_some()
    );
    assert_eq!(
        repo.list_audit("tenant-a", None, None, 20, 0)
            .await
            .expect("audit")
            .total,
        2
    );
}

#[tokio::test]
async fn pagination_filters_literal_search_and_idempotent_migration_preserve_records() {
    let (pool, repo) = setup().await;
    repo.create(&create("r1", "category", "literal_%"))
        .await
        .expect("literal");
    repo.create(&create("r2", "category", "plain"))
        .await
        .expect("plain");
    SqliteMigrationRunner::new(pool)
        .run_migrations()
        .await
        .expect("repeat migration");
    let mut query = filter("tenant-a", "category");
    query.q = Some("_%".into());
    assert_eq!(repo.list(&query).await.expect("literal search").total, 1);
    query.q = None;
    query.limit = 1;
    let first = repo.list(&query).await.expect("first page");
    assert_eq!(first.total, 2);
    assert_eq!(first.data.len(), 1);
    query.offset = 1;
    let second = repo.list(&query).await.expect("second page");
    assert_ne!(first.data[0].id, second.data[0].id);
    query.status = Some("enabled".into());
    assert_eq!(repo.list(&query).await.expect("status filter").total, 0);
}

#[tokio::test]
async fn concurrent_updates_to_one_version_allow_only_one_commit() {
    let (_pool, repo) = setup().await;
    repo.create(&create("r1", "category", "original"))
        .await
        .expect("create");
    let first = update(1, "writer-a");
    let second = update(1, "writer-b");
    let (a, b) = tokio::join!(
        repo.update("tenant-a", "category", "r1", &first),
        repo.update("tenant-a", "category", "r1", &second),
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let rejected = if a.is_err() { a } else { b };
    assert!(matches!(rejected, Err(BackendError::Conflict(_))));
    assert_eq!(
        repo.get("tenant-a", "category", "r1")
            .await
            .expect("get")
            .expect("resource")
            .version,
        2
    );
    assert_eq!(
        repo.list_audit("tenant-a", None, None, 20, 0)
            .await
            .expect("audit")
            .total,
        2
    );
}
