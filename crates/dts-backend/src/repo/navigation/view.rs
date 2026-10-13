use super::*;
use crate::BackendError;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};

const RECOVERY: [(&str, Option<&str>); 4] = [
    ("/system/menus", Some("system:read")),
    ("/system/menu-templates", Some("system:read")),
    ("/system/users", Some("user:manage")),
    ("/account/profile", None),
];
fn can_read(route: &RegisteredNavigationRoute, permissions: &BTreeSet<String>) -> bool {
    route
        .required_permissions
        .iter()
        .all(|permission| permissions.contains(permission))
}
fn catalog_node(route: &RegisteredNavigationRoute, recovery: bool) -> NavigationNode {
    NavigationNode {
        id: format!("catalog:{}", route.id),
        menu_version: None,
        parent_id: None,
        name: route.name.clone(),
        path: route.path.clone(),
        sort_order: route.sort_order,
        icon: None,
        source: "catalog".into(),
        recovery,
    }
}
pub fn effective_navigation(
    params: &ReadNavigationParams,
    permissions: &BTreeSet<String>,
    assignment: Option<&NavigationAssignment>,
    now: DateTime<Utc>,
) -> Result<NavigationView, BackendError> {
    validation::scope(&params.scope)?;
    let mode = match assignment {
        Some(policy) if policy.tenant_id != params.scope.tenant_id => {
            return Err(BackendError::Forbidden("导航策略不属于当前租户".into()));
        },
        Some(policy)
            if policy.status == NavigationAssignmentStatus::Active
                && policy.expires_at.is_some_and(|expiry| expiry <= now) =>
        {
            NavigationMode::Expired
        },
        Some(policy) if policy.status == NavigationAssignmentStatus::Active => {
            NavigationMode::Assigned
        },
        _ => NavigationMode::DefaultCatalog,
    };
    let (mut nodes, filtered_count) = match mode {
        NavigationMode::Assigned => {
            let policy =
                assignment.ok_or_else(|| BackendError::Internal("真实导航分配不存在".into()))?;
            validation::validate_tree(&policy.template.menus)?;
            let by_id: BTreeMap<_, _> = policy
                .template
                .menus
                .iter()
                .map(|menu| (&menu.id, menu))
                .collect();
            let allowed = |menu: &FrozenNavigationMenu| {
                menu.visible
                    && params
                        .registry
                        .route(&menu.path)
                        .is_some_and(|route| can_read(route, permissions))
                    && menu
                        .additional_permission
                        .as_ref()
                        .is_none_or(|permission| permissions.contains(permission))
            };
            let nodes: Vec<_> = policy
                .template
                .menus
                .iter()
                .filter(|menu| {
                    if !allowed(menu) {
                        return false;
                    }
                    let mut parent = menu.parent_id.as_ref();
                    while let Some(id) = parent {
                        let Some(ancestor) = by_id.get(id) else {
                            return false;
                        };
                        if !allowed(ancestor) {
                            return false;
                        }
                        parent = ancestor.parent_id.as_ref();
                    }
                    true
                })
                .map(|menu| NavigationNode {
                    id: menu.id.clone(),
                    menu_version: Some(menu.version),
                    parent_id: menu.parent_id.clone(),
                    name: menu.name.clone(),
                    path: menu.path.clone(),
                    sort_order: menu.sort_order,
                    icon: menu.icon.clone(),
                    source: "template".into(),
                    recovery: false,
                })
                .collect();
            let filtered = policy.template.menus.len() - nodes.len();
            (nodes, filtered as u32)
        },
        NavigationMode::Expired => (
            vec![],
            assignment.map_or(0, |policy| policy.template.menus.len() as u32),
        ),
        NavigationMode::DefaultCatalog => {
            let nodes: Vec<_> = params
                .registry
                .routes()
                .iter()
                .filter(|route| can_read(route, permissions))
                .map(|route| catalog_node(route, false))
                .collect();
            let filtered = params.registry.routes().len() - nodes.len();
            (nodes, filtered as u32)
        },
    };
    for (path, permission) in RECOVERY {
        let Some(route) = params.registry.route(path) else {
            continue;
        };
        if !can_read(route, permissions)
            || permission.is_some_and(|permission| !permissions.contains(permission))
        {
            continue;
        }
        if let Some(node) = nodes.iter_mut().find(|node| node.path == path) {
            node.recovery = true;
        } else {
            nodes.push(catalog_node(route, true));
        }
    }
    nodes.sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
    Ok(NavigationView {
        tenant_id: params.scope.tenant_id.clone(),
        mode,
        assignment_id: assignment.map(|policy| policy.id.clone()),
        assignment_version: assignment.map_or(0, |policy| policy.version),
        template_id: assignment.map(|policy| policy.template.id.clone()),
        template_version: assignment.map(|policy| policy.template.version),
        expires_at: assignment.and_then(|policy| policy.expires_at),
        nodes,
        filtered_count,
        observed_at: now,
        affects_navigation_only: true,
    })
}
