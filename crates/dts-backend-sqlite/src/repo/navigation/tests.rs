mod fixture;
use super::*;
use fixture::Fixture;

#[tokio::test]
async fn default_catalog_is_filtered_by_actual_current_roles_and_inactive_actor_is_denied() {
    let f = Fixture::new().await;
    let view = f.repo.current(&f.read("reader")).await.unwrap();
    assert_eq!(view.mode, NavigationMode::DefaultCatalog);
    assert_eq!(view.assignment_version, 0);
    assert!(
        view.nodes
            .iter()
            .any(|node| node.path == "/development/sql")
    );
    assert!(!view.nodes.iter().any(|node| node.path == "/system/users"));
    sqlx::query("DELETE FROM role_permission WHERE role_id='read'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        !f.repo
            .current(&f.read("reader"))
            .await
            .unwrap()
            .nodes
            .iter()
            .any(|node| node.path == "/development/sql")
    );
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='reader'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.repo.current(&f.read("reader")).await,
        Err(BackendError::Forbidden(_))
    ));
}
#[tokio::test]
async fn assignment_requires_both_permissions_and_never_accepts_cross_tenant_actor() {
    let f = Fixture::new().await;
    for actor in ["reader", "writer", "other-user"] {
        let mut p = f.assign(0, actor);
        p.scope = f.scope(actor);
        assert!(matches!(
            f.repo.assign(&p).await,
            Err(BackendError::Forbidden(_))
        ));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant_navigation_receipt")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn stable_assignment_and_receipt_replay_preserve_original_versions_and_semantics() {
    let f = Fixture::new().await;
    let p = f.assign(0, "initial");
    let first = f.repo.assign(&p).await.unwrap();
    assert!(uuid::Uuid::parse_str(&first.assignment.id).is_ok());
    assert!(uuid::Uuid::parse_str(&first.receipt_id).is_ok());
    let replay = f.repo.assign(&p).await.unwrap();
    assert_eq!(replay.assignment, first.assignment);
    assert_eq!(replay.receipt_id, first.receipt_id);
    assert!(replay.replayed);
    let mut changed = f.assign(0, "initial");
    changed.expires_at = Some(chrono::Utc::now() + chrono::Duration::days(1));
    assert!(matches!(
        f.repo.assign(&changed).await,
        Err(BackendError::Conflict(_))
    ));
    let second = f.repo.assign(&f.assign(1, "second")).await.unwrap();
    assert_eq!(second.assignment.id, first.assignment.id);
    assert_eq!(second.assignment.version, 2);
    let history = f
        .repo
        .history(&NavigationHistoryFilter {
            scope: f.scope("manager"),
            limit: 1,
            offset: 1,
        })
        .await
        .unwrap();
    assert_eq!(history.total, 2);
    assert_eq!(history.data.len(), 1);
    assert!(matches!(
        f.repo
            .history(&NavigationHistoryFilter {
                scope: f.scope("reader"),
                limit: 10,
                offset: 0
            })
            .await,
        Err(BackendError::Forbidden(_))
    ));
}
#[tokio::test]
async fn concurrent_assignment_with_same_cas_version_has_exactly_one_winner() {
    let f = Fixture::new().await;
    let a = f.assign(0, "a");
    let b = f.assign(0, "b");
    let (a, b) = tokio::join!(f.repo.assign(&a), f.repo.assign(&b));
    assert!(matches!(
        (a, b),
        (Ok(_), Err(BackendError::Conflict(_))) | (Err(BackendError::Conflict(_)), Ok(_))
    ));
    let history = f
        .repo
        .history(&NavigationHistoryFilter {
            scope: f.scope("manager"),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(history.total, 1);
}
#[tokio::test]
async fn definition_update_has_no_effect_until_explicit_reassignment() {
    let f = Fixture::new().await;
    let first = f.repo.assign(&f.assign(0, "initial")).await.unwrap();
    sqlx::query("UPDATE product_resource SET name='Renamed',properties=json_set(properties,'$.visible',json('false')),version=version+1 WHERE id='sql-menu'")
        .execute(&f.pool).await.unwrap();
    let view = f.repo.current(&f.read("reader")).await.unwrap();
    let node = view
        .nodes
        .iter()
        .find(|node| node.id == "sql-menu")
        .unwrap();
    assert_eq!(node.name, "sql-menu");
    assert_eq!(node.menu_version, Some(1));
    let second = f.repo.assign(&f.assign(1, "refresh")).await.unwrap();
    assert_eq!(first.assignment.template.menus[0].version, 1);
    assert_eq!(second.assignment.template.menus[0].version, 2);
    assert!(
        !f.repo
            .current(&f.read("reader"))
            .await
            .unwrap()
            .nodes
            .iter()
            .any(|node| node.id == "sql-menu")
    );
}
#[tokio::test]
async fn expiry_does_not_fall_back_and_explicit_revoke_restores_catalog_with_recovery_paths() {
    let f = Fixture::new().await;
    let mut p = f.assign(0, "expires");
    p.expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(1));
    let active = f.repo.assign(&p).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let expired = f.repo.current(&f.read("reader")).await.unwrap();
    assert_eq!(expired.mode, NavigationMode::Expired);
    assert!(
        !expired
            .nodes
            .iter()
            .any(|node| node.path == "/development/sql")
    );
    let recovery = f.repo.current(&f.read("manager")).await.unwrap();
    for path in [
        "/system/menus",
        "/system/menu-templates",
        "/system/users",
        "/account/profile",
    ] {
        assert!(
            recovery
                .nodes
                .iter()
                .any(|node| node.recovery && node.path == path)
        );
    }
    let revoke = RevokeNavigationParams {
        scope: f.scope("manager"),
        assignment_id: active.assignment.id,
        expected_assignment_version: 1,
        idempotency_key: "revoke".into(),
        reason: "Return to catalog".into(),
    };
    let revoked = f.repo.revoke(&revoke).await.unwrap();
    assert_eq!(revoked.assignment.version, 2);
    assert!(f.repo.revoke(&revoke).await.unwrap().replayed);
    let default = f.repo.current(&f.read("reader")).await.unwrap();
    assert_eq!(default.mode, NavigationMode::DefaultCatalog);
    assert!(
        default
            .nodes
            .iter()
            .any(|node| node.path == "/development/sql")
    );
    assert_eq!(default.assignment_version, 2);
}
#[tokio::test]
async fn receipt_failure_rolls_back_assignment_and_history_fks_keep_real_definitions() {
    let f = Fixture::new().await;
    sqlx::raw_sql(
        "CREATE TRIGGER fail_navigation_receipt BEFORE INSERT ON tenant_navigation_receipt
        BEGIN SELECT RAISE(ABORT,'receipt failure'); END;",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    assert!(f.repo.assign(&f.assign(0, "failed")).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenant_navigation_assignment")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_navigation_receipt")
        .execute(&f.pool)
        .await
        .unwrap();
    f.repo.assign(&f.assign(0, "saved")).await.unwrap();
    for sql in [
        "DELETE FROM product_resource WHERE id='template'",
        "DELETE FROM product_resource WHERE id='sql-menu'",
        "DELETE FROM tenant_navigation_receipt",
        "UPDATE tenant_navigation_receipt SET action='revoke'",
        "DELETE FROM tenant_navigation_assignment",
    ] {
        assert!(sqlx::query(sql).execute(&f.pool).await.is_err(), "{sql}");
    }
}
#[tokio::test]
async fn assignment_rejects_unclosed_parent_tree_invalid_routes_and_changed_template_version() {
    let f = Fixture::new().await;
    f.menu("child", Some("sql-menu"), "/development/sql").await;
    f.template("partial", &["child"], None).await;
    let mut p = f.assign(0, "partial");
    p.template_id = "partial".into();
    assert!(f.repo.assign(&p).await.is_err());
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.path','https://evil.test') WHERE id='sql-menu'")
        .execute(&f.pool).await.unwrap();
    assert!(f.repo.assign(&f.assign(0, "invalid")).await.is_err());
    let mut p = f.assign(0, "wrongversion");
    p.expected_template_version = 2;
    assert!(matches!(
        f.repo.assign(&p).await,
        Err(BackendError::Conflict(_))
    ));
}
