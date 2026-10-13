use super::*;
use crate::ResourceReference;
use chrono::TimeZone;
use serde_json::json;
use std::sync::Arc;
fn routes() -> Arc<NavigationRegistry> {
    Arc::new(
        NavigationRegistry::new(vec![
            RegisteredNavigationRoute {
                id: "sql".into(),
                path: "/development/sql".into(),
                name: "SQL".into(),
                required_permissions: vec!["development:read".into()],
                sort_order: 0,
            },
            RegisteredNavigationRoute {
                id: "profile".into(),
                path: "/account/profile".into(),
                name: "Profile".into(),
                required_permissions: vec![],
                sort_order: 1,
            },
        ])
        .unwrap(),
    )
}
fn menu(id: &str, parent: Option<&str>, path: &str) -> ProductResource {
    let now = Utc::now();
    ProductResource {
        id: id.into(),
        tenant_id: "tenant".into(),
        kind: "system_menu".into(),
        code: id.into(),
        name: id.into(),
        description: None,
        parent_id: parent.map(str::to_owned),
        status: "draft".into(),
        properties: json!({"path":path,"visible":true}),
        version: 1,
        created_by: "manager".into(),
        created_at: now,
        updated_at: now,
        references: vec![],
    }
}
fn template(ids: &[&str]) -> ProductResource {
    let mut resource = menu("template", None, "/development/sql");
    resource.kind = "menu_template".into();
    resource.properties = json!({"menu_ids":ids});
    resource.references = ids
        .iter()
        .map(|id| ResourceReference {
            field: "menu_ids".into(),
            target_kind: "system_menu".into(),
            target_id: (*id).into(),
        })
        .collect();
    resource
}
#[test]
fn freeze_requires_registered_routes_current_tenant_and_actual_reference_set() {
    let registered = routes();
    let mut actual = menu("sql", None, "/development/sql");
    assert!(
        freeze(
            "tenant",
            &template(&["sql"]),
            &[actual.clone()],
            &registered
        )
        .is_ok()
    );
    for path in [
        "https://evil.example",
        "//evil",
        "/development/sql?admin=true",
        "/unknown",
    ] {
        actual.properties["path"] = json!(path);
        assert!(
            freeze(
                "tenant",
                &template(&["sql"]),
                &[actual.clone()],
                &registered
            )
            .is_err()
        );
    }
    actual.properties["path"] = json!("/development/sql");
    actual.tenant_id = "other".into();
    assert!(freeze("tenant", &template(&["sql"]), &[actual], &registered).is_err());
    let mut source = template(&["sql"]);
    source.references.clear();
    assert!(
        freeze(
            "tenant",
            &source,
            &[menu("sql", None, "/development/sql")],
            &registered
        )
        .is_err()
    );
}
#[test]
fn freeze_rejects_missing_parent_cycles_duplicate_ids_and_unregistered_extra_permission() {
    let registered = routes();
    let child = menu("child", Some("parent"), "/development/sql");
    assert!(
        freeze(
            "tenant",
            &template(&["child"]),
            &[child.clone()],
            &registered
        )
        .is_err()
    );
    let parent = menu("parent", Some("child"), "/development/sql");
    assert!(
        freeze(
            "tenant",
            &template(&["parent", "child"]),
            &[parent, child],
            &registered
        )
        .is_err()
    );
    assert!(
        freeze(
            "tenant",
            &template(&["sql", "sql"]),
            &[
                menu("sql", None, "/development/sql"),
                menu("sql", None, "/development/sql")
            ],
            &registered
        )
        .is_err()
    );
    let mut bad = menu("sql", None, "/development/sql");
    bad.properties["permission"] = json!("root:all");
    assert!(freeze("tenant", &template(&["sql"]), &[bad], &registered).is_err());
}
#[test]
fn deadline_uses_exact_clock_and_explicit_expiry_can_only_shorten_default() {
    let now = Utc.with_ymd_and_hms(2027, 1, 2, 3, 4, 5).unwrap();
    assert_eq!(deadline(now, None, None).unwrap(), None);
    assert_eq!(
        deadline(now, Some(2), None).unwrap(),
        Some(now + Duration::days(2))
    );
    assert!(deadline(now, Some(2), Some(now + Duration::days(3))).is_err());
    assert!(deadline(now, None, Some(now)).is_err());
    assert!(deadline(now, Some(0), None).is_err());
}
#[test]
fn assigned_policy_never_grants_api_permission_or_falls_back_after_expiry() {
    let now = Utc.with_ymd_and_hms(2027, 1, 2, 3, 4, 5).unwrap();
    let registry = routes();
    let frozen = freeze(
        "tenant",
        &template(&["sql"]),
        &[menu("sql", None, "/development/sql")],
        &registry,
    )
    .unwrap();
    let policy = NavigationAssignment {
        id: "assignment".into(),
        tenant_id: "tenant".into(),
        version: 1,
        status: NavigationAssignmentStatus::Active,
        template: frozen,
        assigned_by: "manager".into(),
        assigned_at: now - Duration::days(1),
        expires_at: Some(now),
        revoked_by: None,
        revoked_at: None,
    };
    let request = ReadNavigationParams {
        scope: NavigationScope {
            tenant_id: "tenant".into(),
            actor_id: "reader".into(),
        },
        registry,
    };
    let permissions = BTreeSet::from(["development:read".into()]);
    let result = effective_navigation(&request, &permissions, Some(&policy), now).unwrap();
    assert_eq!(result.mode, NavigationMode::Expired);
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].path, "/account/profile");
    let mut active = policy.clone();
    active.expires_at = Some(now + Duration::days(1));
    let denied = effective_navigation(&request, &BTreeSet::new(), Some(&active), now).unwrap();
    assert!(
        !denied
            .nodes
            .iter()
            .any(|node| node.path == "/development/sql")
    );
    assert!(denied.affects_navigation_only);
}
#[test]
fn hidden_or_unauthorized_parent_hides_child_even_when_child_route_is_allowed() {
    let registered = routes();
    let mut parent = menu("parent", None, "/development/sql");
    parent.properties["visible"] = json!(false);
    let child = menu("child", Some("parent"), "/account/profile");
    let frozen = freeze(
        "tenant",
        &template(&["parent", "child"]),
        &[parent, child],
        &registered,
    )
    .unwrap();
    let now = Utc::now();
    let policy = NavigationAssignment {
        id: "assignment".into(),
        tenant_id: "tenant".into(),
        version: 1,
        status: NavigationAssignmentStatus::Active,
        template: frozen,
        assigned_by: "manager".into(),
        assigned_at: now,
        expires_at: None,
        revoked_by: None,
        revoked_at: None,
    };
    let request = ReadNavigationParams {
        scope: NavigationScope {
            tenant_id: "tenant".into(),
            actor_id: "reader".into(),
        },
        registry: registered,
    };
    let result = effective_navigation(&request, &BTreeSet::new(), Some(&policy), now).unwrap();
    assert!(!result.nodes.iter().any(|node| node.id == "child"));
    assert_eq!(result.filtered_count, 2);
    assert!(
        result
            .nodes
            .iter()
            .any(|node| node.recovery && node.path == "/account/profile")
    );
}
