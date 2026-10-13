// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::product_resource::*, repo::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{
    SqliteDatasourceRepo, SqliteMigrationRunner, SqliteProductResourceRepo, SqliteRoleRepo,
    SqliteUserRepo,
};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

async fn setup() -> (SqlitePool, SqliteProductResourceRepo) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("SQLite");
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .expect("migrations");
    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../migrations/004_product_external_refs.sql"))
            .execute(&pool)
            .await
            .expect("idempotent external migration");
    }
    for tenant in ["a", "b"] {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?1, ?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("tenant");
        sqlx::query("INSERT INTO \"user\" (id, tenant_id, username, email, password_hash) VALUES (?1, ?2, ?1, ?1, 'hash')")
            .bind(format!("user-{tenant}")).bind(tenant).execute(&pool).await.expect("user");
        sqlx::query("INSERT INTO role (id, tenant_id, name) VALUES (?1, ?2, ?1)")
            .bind(format!("role-{tenant}"))
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("role");
        sqlx::query("INSERT INTO datasource (id, tenant_id, name, db_type, host, port, created_by) VALUES (?1, ?2, ?1, 'mysql', 'localhost', 3306, 'actor')")
            .bind(format!("ds-{tenant}")).bind(tenant).execute(&pool).await.expect("source");
    }
    let repo = SqliteProductResourceRepo::new(pool.clone());
    (pool, repo)
}

fn fields(code: &str, references: Vec<ResourceReference>) -> ResourceWrite {
    ResourceWrite {
        code: code.into(),
        name: code.into(),
        description: None,
        parent_id: None,
        status: "draft".into(),
        properties: json!({}),
        references,
    }
}

fn reference(field: &str, kind: &str, target: &str) -> ResourceReference {
    ResourceReference {
        field: field.into(),
        target_kind: kind.into(),
        target_id: target.into(),
    }
}

fn create(id: &str, kind: &str, references: Vec<ResourceReference>) -> CreateResourceParams {
    CreateResourceParams {
        id: id.into(),
        tenant_id: "a".into(),
        kind: kind.into(),
        actor_id: "user-a".into(),
        fields: fields(id, references),
    }
}

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn real_identity_source_and_internal_references_roundtrip_without_mirrors() {
    let (pool, repo) = setup().await;
    repo.create(&create("category", "warehouse_category", vec![]))
        .await
        .expect("category");
    let links = vec![
        reference("category_id", "warehouse_category", "category"),
        reference("manager_id", "user", "user-a"),
        reference("authorized_role_ids", "role", "role-a"),
        reference("source_id", "datasource", "ds-a"),
    ];
    let saved = repo
        .create(&create("config", "organization", links.clone()))
        .await
        .expect("config");
    assert_eq!(saved.references.len(), links.len());
    for link in links {
        assert!(saved.references.contains(&link));
    }
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM product_resource").await,
        2
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM product_resource_links").await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM product_resource_external_links"
        )
        .await,
        3
    );
    assert_eq!(
        repo.get("a", "organization", "config")
            .await
            .expect("get")
            .expect("exists")
            .references,
        saved.references
    );
}

#[tokio::test]
async fn external_references_reject_cross_tenant_missing_kind_and_disabled_users_atomically() {
    let (pool, repo) = setup().await;
    sqlx::query("UPDATE \"user\" SET status = 'disabled' WHERE id = 'user-a'")
        .execute(&pool)
        .await
        .expect("disable");
    for (kind, id) in [
        ("user", "user-b"),
        ("role", "role-b"),
        ("datasource", "ds-b"),
        ("user", "user-a"),
        ("datasource", "missing"),
        ("datasource", "user-a"),
    ] {
        let result = repo
            .create(&create(
                "bad",
                "organization",
                vec![reference("id", kind, id)],
            ))
            .await;
        assert!(
            matches!(result, Err(BackendError::ConstraintViolation(_))),
            "{kind}/{id}"
        );
    }
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM product_resource").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM product_resource_audit").await,
        0
    );
}

#[tokio::test]
async fn disabling_keeps_membership_but_soft_delete_requires_releasing_reference() {
    let (pool, repo) = setup().await;
    repo.create(&create(
        "group",
        "security_user_group",
        vec![reference("member_ids", "user", "user-a")],
    ))
    .await
    .expect("group");
    sqlx::query("UPDATE \"user\" SET status = 'disabled' WHERE id = 'user-a'")
        .execute(&pool)
        .await
        .expect("disable retains relation");
    let linked_status: String =
        sqlx::query_scalar("SELECT user_status FROM product_resource_external_links")
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(linked_status, "disabled");
    let users = SqliteUserRepo::new(pool.clone());
    assert!(
        users
            .update(
                "user-a",
                &dts_backend::types::UpdateUserParams {
                    display_name: Some("must roll back".into()),
                    status: Some("deleted".into()),
                }
            )
            .await
            .is_err()
    );
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM \"user\" WHERE id = 'user-a'")
            .fetch_one(&pool)
            .await
            .expect("display name");
    assert!(display_name.is_none());
    assert!(matches!(
        users.soft_delete("user-a").await,
        Err(BackendError::ConstraintViolation(_))
    ));
    let status: String = sqlx::query_scalar("SELECT status FROM \"user\" WHERE id = 'user-a'")
        .fetch_one(&pool)
        .await
        .expect("user state");
    assert_eq!(status, "disabled");
    repo.update(
        "a",
        "security_user_group",
        "group",
        &UpdateResourceParams {
            expected_version: 1,
            actor_id: "actor".into(),
            fields: fields("group", vec![]),
        },
    )
    .await
    .expect("remove member");
    users
        .soft_delete("user-a")
        .await
        .expect("delete after releasing relation");
}

#[tokio::test]
async fn referenced_role_delete_rolls_back_permissions_and_memberships() {
    let (pool, repo) = setup().await;
    sqlx::query("INSERT INTO user_role (user_id, role_id) VALUES ('user-a', 'role-a')")
        .execute(&pool)
        .await
        .expect("membership");
    sqlx::query("INSERT INTO role_permission (role_id, permission) VALUES ('role-a', 'task:view')")
        .execute(&pool)
        .await
        .expect("permission");
    repo.create(&create(
        "portal",
        "portal",
        vec![reference("authorized_role_ids", "role", "role-a")],
    ))
    .await
    .expect("portal");
    let roles = SqliteRoleRepo::new(pool.clone());
    assert!(matches!(
        roles.delete("role-a").await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM role_permission WHERE role_id = 'role-a'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM user_role WHERE role_id = 'role-a'"
        )
        .await,
        1
    );
    repo.delete("a", "portal", "portal", 1, "actor")
        .await
        .expect("remove portal");
    roles.delete("role-a").await.expect("remove role");
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM role_permission WHERE role_id = 'role-a'"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn datasource_delete_rejects_real_configuration_reference() {
    let (pool, repo) = setup().await;
    repo.create(&create(
        "collector",
        "metadata_collector",
        vec![reference("source_id", "datasource", "ds-a")],
    ))
    .await
    .expect("collector");
    let sources = SqliteDatasourceRepo::new(pool.clone());
    assert!(matches!(
        sources.delete("ds-a", "a").await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM datasource WHERE id = 'ds-a'").await,
        1
    );
    repo.delete("a", "metadata_collector", "collector", 1, "actor")
        .await
        .expect("delete collector");
    assert!(sources.delete("ds-a", "a").await.expect("source delete"));
}

#[tokio::test]
async fn direct_sql_cannot_insert_untyped_null_or_cross_tenant_external_reference() {
    let (pool, repo) = setup().await;
    repo.create(&create("config", "organization", vec![]))
        .await
        .expect("config");
    for sql in [
        "INSERT INTO product_resource_external_links (tenant_id, source_kind, source_id, field, target_kind, target_id) VALUES ('a', 'organization', 'config', 'field', 'role', 'role-a')",
        "INSERT INTO product_resource_external_links (tenant_id, source_kind, source_id, field, target_kind, target_id, role_id) VALUES ('a', 'organization', 'config', 'field', 'role', 'role-b', 'role-b')",
        "INSERT INTO product_resource_external_links (tenant_id, source_kind, source_id, field, target_kind, target_id, user_id, user_status) VALUES ('a', 'organization', 'config', 'field', 'user', 'user-a', 'user-a', 'deleted')",
        "INSERT INTO product_resource_external_links (tenant_id, source_kind, source_id, field, target_kind, target_id, role_id, datasource_id) VALUES ('a', 'organization', 'config', 'field', 'role', 'role-a', 'role-a', 'ds-a')",
    ] {
        assert!(sqlx::query(sql).execute(&pool).await.is_err(), "{sql}");
    }
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM product_resource_external_links"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn actor_filter_applies_before_pagination_and_matches_count() {
    let (_pool, repo) = setup().await;
    for index in 0..5 {
        let mut request = create(&format!("req-{index}"), "data_access_request", vec![]);
        request.actor_id = if index % 2 == 0 { "actor-a" } else { "actor-b" }.into();
        repo.create(&request).await.expect("request");
    }
    let filter = ProductResourceFilter {
        tenant_id: "a".into(),
        kind: "data_access_request".into(),
        q: None,
        status: None,
        created_by: Some("actor-a".into()),
        limit: 1,
        offset: 1,
    };
    let page = repo.list(&filter).await.expect("filtered page");
    assert_eq!(page.total, 3);
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].created_by, "actor-a");
}

#[tokio::test]
async fn concurrent_composite_dependencies_cannot_create_cycle() {
    let (_pool, repo) = setup().await;
    for id in ["metric-a", "metric-b"] {
        repo.create(&create(id, "composite_metric", vec![]))
            .await
            .expect("metric");
    }
    let a = UpdateResourceParams {
        expected_version: 1,
        actor_id: "actor".into(),
        fields: fields(
            "metric-a",
            vec![reference("metric_ids", "composite_metric", "metric-b")],
        ),
    };
    let b = UpdateResourceParams {
        expected_version: 1,
        actor_id: "actor".into(),
        fields: fields(
            "metric-b",
            vec![reference("metric_ids", "composite_metric", "metric-a")],
        ),
    };
    let (a, b) = tokio::join!(
        repo.update("a", "composite_metric", "metric-a", &a),
        repo.update("a", "composite_metric", "metric-b", &b),
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert!(
        matches!(a, Err(BackendError::ConstraintViolation(_)))
            || matches!(b, Err(BackendError::ConstraintViolation(_)))
    );
    let audits = repo
        .list_audit("a", Some("composite_metric"), None, 20, 0)
        .await
        .expect("audit");
    assert_eq!(audits.total, 3);
}
