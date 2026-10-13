use super::*;
use crate::{BackendError, PERMISSIONS, ProductResource};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}
pub fn bounded(value: &str, max: usize) -> Result<(), BackendError> {
    if value.trim().is_empty() || value.chars().count() > max || value.chars().any(char::is_control)
    {
        Err(invalid("导航标识或显示文字无效"))
    } else {
        Ok(())
    }
}
pub fn registry(routes: &[RegisteredNavigationRoute]) -> Result<(), BackendError> {
    if routes.is_empty() || routes.len() > 1000 {
        return Err(invalid("注册导航目录数量无效"));
    }
    let mut paths = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for route in routes {
        bounded(&route.id, 128)?;
        bounded(&route.name, 128)?;
        safe_path(&route.path)?;
        if !paths.insert(&route.path) || !ids.insert(&route.id) || route.sort_order > 1_000_000 {
            return Err(invalid("注册导航目录身份重复或排序无效"));
        }
        if route.required_permissions.len() > 20
            || route
                .required_permissions
                .iter()
                .any(|permission| !PERMISSIONS.contains(&permission.as_str()))
        {
            return Err(invalid("导航读权限不属于注册权限"));
        }
    }
    Ok(())
}
fn safe_path(path: &str) -> Result<(), BackendError> {
    if path.len() > 256
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.chars().any(|value| {
            value.is_control()
                || value.is_whitespace()
                || matches!(value, '?' | '#' | '\\' | ':' | '%' | '<')
        })
    {
        return Err(invalid("导航路径必须为受控站内注册路径"));
    }
    Ok(())
}
pub fn scope(scope: &NavigationScope) -> Result<(), BackendError> {
    bounded(&scope.tenant_id, 128)?;
    bounded(&scope.actor_id, 128)
}
pub fn assignment_version(version: i64, initial: bool) -> Result<(), BackendError> {
    if version < if initial { 0 } else { 1 } || version == i64::MAX {
        Err(invalid("导航分配版本无效"))
    } else {
        Ok(())
    }
}
pub fn assign_semantics(p: &AssignNavigationParams) -> Result<Value, BackendError> {
    scope(&p.scope)?;
    bounded(&p.template_id, 128)?;
    bounded(&p.idempotency_key, 128)?;
    assignment_version(p.expected_template_version, false)?;
    assignment_version(p.expected_assignment_version, true)?;
    Ok(
        json!({"action":"assign","actor_id":p.scope.actor_id,"template_id":p.template_id,
        "expected_template_version":p.expected_template_version,"expected_assignment_version":p.expected_assignment_version,
        "requested_expires_at":p.expires_at}),
    )
}
pub fn revoke_semantics(p: &RevokeNavigationParams) -> Result<Value, BackendError> {
    scope(&p.scope)?;
    bounded(&p.assignment_id, 128)?;
    bounded(&p.idempotency_key, 128)?;
    bounded(&p.reason, 8192)?;
    assignment_version(p.expected_assignment_version, false)?;
    Ok(
        json!({"action":"revoke","actor_id":p.scope.actor_id,"assignment_id":p.assignment_id,
        "expected_assignment_version":p.expected_assignment_version,"reason":p.reason}),
    )
}
pub fn deadline(
    now: DateTime<Utc>,
    days: Option<u32>,
    requested: Option<DateTime<Utc>>,
) -> Result<Option<DateTime<Utc>>, BackendError> {
    let default = days
        .map(|days| {
            if !(1..=36500).contains(&days) {
                return Err(invalid("模板有效天数必须为1至36500"));
            }
            now.checked_add_signed(Duration::days(i64::from(days)))
                .ok_or_else(|| invalid("模板期限超过时间容量"))
        })
        .transpose()?;
    if let Some(requested) = requested {
        if requested <= now || default.is_some_and(|maximum| requested > maximum) {
            return Err(invalid("显式失效时间必须在未来且不超过模板默认期限"));
        }
        Ok(Some(requested))
    } else {
        Ok(default)
    }
}
pub fn freeze(
    tenant: &str,
    template: &ProductResource,
    menus: &[ProductResource],
    registry: &NavigationRegistry,
) -> Result<NavigationTemplateSnapshot, BackendError> {
    if template.tenant_id != tenant
        || template.kind != "menu_template"
        || template.status != "draft"
    {
        return Err(invalid("导航模板不是当前租户的有效草稿定义"));
    }
    bounded(&template.name, 128)?;
    assignment_version(template.version, false)?;
    let ids: Vec<String> = serde_json::from_value(template.properties["menu_ids"].clone())
        .map_err(|_| invalid("模板必须包含真实菜单ID数组"))?;
    let desired: BTreeSet<_> = ids.iter().cloned().collect();
    let references: BTreeSet<_> = template
        .references
        .iter()
        .filter(|reference| reference.field == "menu_ids" && reference.target_kind == "system_menu")
        .map(|reference| reference.target_id.clone())
        .collect();
    if ids.is_empty()
        || ids.len() > 1000
        || desired.len() != ids.len()
        || desired != references
        || menus.len() != ids.len()
    {
        return Err(invalid("模板菜单数量、身份或真实引用不一致"));
    }
    let validity_days = template
        .properties
        .get("validity_days")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .filter(|days| (1..=36500).contains(days))
                .map(|days| days as u32)
                .ok_or_else(|| invalid("模板有效天数必须为1至36500"))
        })
        .transpose()?;
    let edition = optional_text(&template.properties, "edition", 64)?;
    let mut frozen = Vec::new();
    let mut seen = BTreeSet::new();
    for menu in menus {
        if menu.tenant_id != tenant
            || menu.kind != "system_menu"
            || menu.status != "draft"
            || !desired.contains(&menu.id)
            || !seen.insert(&menu.id)
        {
            return Err(invalid("模板菜单定义不属于当前租户或身份重复"));
        }
        bounded(&menu.name, 128)?;
        assignment_version(menu.version, false)?;
        let path = menu.properties["path"]
            .as_str()
            .ok_or_else(|| invalid("菜单缺少页面路径"))?;
        safe_path(path)?;
        if registry.route(path).is_none() {
            return Err(invalid("菜单路径没有实际注册"));
        }
        let additional_permission = optional_text(&menu.properties, "permission", 128)?;
        if additional_permission
            .as_ref()
            .is_some_and(|permission| !PERMISSIONS.contains(&permission.as_str()))
        {
            return Err(invalid("菜单附加权限没有实际注册"));
        }
        let visible = menu
            .properties
            .get("visible")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid("菜单可见性必须为布尔值"))
            })
            .transpose()?
            .unwrap_or(true);
        let sort_order = menu
            .properties
            .get("sort_order")
            .map(|value| {
                value
                    .as_u64()
                    .filter(|order| *order <= 1_000_000)
                    .map(|order| order as u32)
                    .ok_or_else(|| invalid("菜单排序无效"))
            })
            .transpose()?
            .unwrap_or(0);
        let icon = optional_text(&menu.properties, "icon", 64)?;
        if icon.as_ref().is_some_and(|icon| {
            !icon
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-'))
        }) {
            return Err(invalid("菜单图标只能使用安全图标标识"));
        }
        frozen.push(FrozenNavigationMenu {
            id: menu.id.clone(),
            version: menu.version,
            parent_id: menu.parent_id.clone(),
            name: menu.name.clone(),
            path: path.into(),
            additional_permission,
            visible,
            sort_order,
            icon,
        });
    }
    validate_tree(&frozen)?;
    frozen.sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
    Ok(NavigationTemplateSnapshot {
        id: template.id.clone(),
        version: template.version,
        name: template.name.clone(),
        edition,
        validity_days,
        menus: frozen,
    })
}
fn optional_text(
    properties: &Value,
    key: &str,
    max: usize,
) -> Result<Option<String>, BackendError> {
    properties
        .get(key)
        .filter(|value| !value.is_null())
        .map(|value| {
            let text = value
                .as_str()
                .ok_or_else(|| invalid("导航可选文字必须为字符串"))?;
            if text.is_empty() {
                return Ok(None);
            }
            bounded(text, max)?;
            Ok(Some(text.into()))
        })
        .transpose()
        .map(Option::flatten)
}
pub fn validate_tree(menus: &[FrozenNavigationMenu]) -> Result<(), BackendError> {
    let by_id: BTreeMap<_, _> = menus.iter().map(|menu| (&menu.id, menu)).collect();
    if by_id.len() != menus.len() || menus.len() > 1000 {
        return Err(invalid("菜单树身份重复或超出容量"));
    }
    for menu in menus {
        let mut visited = BTreeSet::from([&menu.id]);
        let mut parent = menu.parent_id.as_ref();
        while let Some(id) = parent {
            if !visited.insert(id) {
                return Err(invalid("菜单父子关系不能成环"));
            }
            let ancestor = by_id
                .get(id)
                .ok_or_else(|| invalid("模板必须包含所选菜单的全部父节点"))?;
            parent = ancestor.parent_id.as_ref();
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
