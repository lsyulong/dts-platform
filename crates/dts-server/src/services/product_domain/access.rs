mod common;
mod portal;
mod template;

use crate::{models::error::AppError, services::product::catalog};
use dts_backend::{PERMISSIONS, ResourceReference};
use serde_json::{Value, json};

pub use template::{RenderedMessage, render_template};

pub fn validate(kind: &str, properties: &Value) -> Result<(), AppError> {
    match kind {
        "portal" => portal::validate(properties),
        "message_template" => template::validate(properties),
        "system_message" => validate_message(properties),
        "announcement" => validate_announcement(properties),
        "organization" => {
            common::object(properties, "properties")?;
            common::optional_text(properties, "manager_user_id", 128)?;
            common::string_list(properties, "member_ids", false, 0, 1000, false)?;
            sort_order(properties)
        },
        "system_menu" => validate_menu(properties),
        "menu_template" => {
            common::object(properties, "properties")?;
            common::string_list(properties, "menu_ids", true, 1, 1000, false)?;
            common::optional_text(properties, "edition", 64)?;
            if let Some(value) = properties.get("validity_days") {
                if value
                    .as_i64()
                    .is_none_or(|days| !(1..=36500).contains(&days))
                {
                    return Err(common::bad("模板有效天数必须是 1 至 36500 的整数"));
                }
            }
            Ok(())
        },
        "system_dictionary" => {
            common::text(properties, "scope", 128)?;
            super::warehouse::validate("enum_standard", properties)
        },
        "tag" => validate_tag(properties),
        "alert_policy" => validate_alert_policy(properties),
        _ => Ok(()),
    }
}

fn validate_alert_policy(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::allowed_fields(
        properties,
        &[
            "event_type",
            "enabled",
            "window_seconds",
            "threshold",
            "message_template_id",
            "recipient_ids",
            "cooldown_seconds",
            "evaluation_interval_seconds",
        ],
        "properties",
    )?;
    if !matches!(
        common::text(properties, "event_type", 32)?,
        "task_failure" | "quality_failure" | "system_health"
    ) {
        return Err(common::bad("告警事件类型无效"));
    }
    common::boolean(properties, "enabled", false)?;
    let integer = |field: &str, min: i64, max: i64| -> Result<i64, AppError> {
        let value = properties
            .get(field)
            .and_then(Value::as_i64)
            .ok_or_else(|| common::bad(format!("{field} 必须是整数")))?;
        if !(min..=max).contains(&value) {
            return Err(common::bad(format!("{field} 必须在 {min} 至 {max} 之间")));
        }
        Ok(value)
    };
    integer("window_seconds", 60, 604_800)?;
    integer("threshold", 1, 1_000_000)?;
    integer("cooldown_seconds", 0, 604_800)?;
    if properties.get("evaluation_interval_seconds").is_some() {
        let interval = integer("evaluation_interval_seconds", 60, 86_400)?;
        let window = integer("window_seconds", 60, 604_800)?;
        if interval > window {
            return Err(common::bad("evaluation_interval_seconds 不能超过统计窗口"));
        }
    }
    common::optional_text(properties, "message_template_id", 128)?;
    common::string_list(properties, "recipient_ids", true, 1, 1000, false)?;
    Ok(())
}

fn validate_tag(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::allowed_fields(properties, &["color", "scope"], "properties")?;
    let color = common::text(properties, "color", 7)?;
    if color.len() != 7
        || !color.starts_with('#')
        || !color[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(common::bad("标签颜色必须是 #RRGGBB"));
    }
    let scope = common::text(properties, "scope", 64)?;
    if scope != "all"
        && !catalog::modules()?
            .iter()
            .any(|module| module.group_id == scope)
    {
        return Err(common::bad("标签适用范围必须是 all 或已注册产品分组"));
    }
    Ok(())
}

pub fn action(
    kind: &str,
    properties: &Value,
    action: &str,
    payload: &Value,
) -> Result<Option<Value>, AppError> {
    match (kind, action) {
        ("portal", "preview") => {
            validate(kind, properties)?;
            Ok(Some(
                json!({"layout":properties["layout"], "theme":properties.get("theme"),
                "authorized_role_ids":properties["authorized_role_ids"], "preview_only":true,
                "data_loaded":false}),
            ))
        },
        ("message_template", "validate") => {
            validate(kind, properties)?;
            Ok(Some(
                json!({"valid":true,"variables":properties.get("variables"),"rendered":false}),
            ))
        },
        ("message_template", "preview") => {
            common::object(payload, "payload")?;
            let empty = json!({});
            let result = render_template(properties, payload.get("parameters").unwrap_or(&empty))?;
            Ok(Some(
                json!({"subject":result.subject,"body":result.body,"format":"plain_text","delivered":false}),
            ))
        },
        ("portal" | "announcement", "publish" | "withdraw" | "authorize") => {
            validate(kind, properties)?;
            Ok(None)
        },
        ("system_message", "send" | "retry" | "mark_read") => {
            Err(common::bad("消息投递和已读必须通过真实收件服务处理"))
        },
        _ => Ok(None),
    }
}

fn sort_order(properties: &Value) -> Result<(), AppError> {
    if properties.get("sort_order").is_some_and(|value| {
        value
            .as_i64()
            .is_none_or(|order| !(0..=1_000_000).contains(&order))
    }) {
        return Err(common::bad("sort_order 必须是 0 至 1000000 的整数"));
    }
    Ok(())
}

fn validate_menu(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    let path = common::text(properties, "path", 256)?;
    if !catalog::modules()?.iter().any(|module| module.path == path) {
        return Err(common::bad("菜单路径必须是实际注册的产品页面路径"));
    }
    if let Some(permission) = common::optional_text(properties, "permission", 128)? {
        if !PERMISSIONS.contains(&permission) {
            return Err(common::bad("菜单权限必须来自后端实际注册权限列表"));
        }
    }
    common::optional_text(properties, "icon", 64)?;
    common::boolean(properties, "visible", true)?;
    sort_order(properties)
}

fn validate_message(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::string_list(properties, "recipient_ids", true, 1, 1000, false)?;
    common::text(properties, "content", 65_536)?;
    common::optional_text(properties, "template_id", 128)?;
    template::channel(properties)?;
    if let Some(parameters) = properties.get("parameters") {
        template::parameters(parameters)?;
    }
    Ok(())
}

fn validate_announcement(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::text(properties, "content", 65_536)?;
    audience_references(properties)?;
    let parse = |key| -> Result<Option<chrono::DateTime<chrono::Utc>>, AppError> {
        common::optional_text(properties, key, 64)?
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map(|time| time.with_timezone(&chrono::Utc))
                    .map_err(|_| common::bad("公告时间必须是带时区的 RFC3339 时间"))
            })
            .transpose()
    };
    let start = parse("publish_at")?;
    let end = parse("expires_at")?;
    if start.zip(end).is_some_and(|(start, end)| start >= end) {
        return Err(common::bad("公告有效期截止必须晚于发布时间"));
    }
    Ok(())
}

pub fn audience_references(properties: &Value) -> Result<Vec<ResourceReference>, AppError> {
    let audience = properties
        .get("audience")
        .ok_or_else(|| common::bad("缺少公告接收范围"))?;
    common::object(audience, "audience")?;
    let audience_type = common::text(audience, "type", 32)?;
    let (field, kind) = match audience_type {
        "tenant" => {
            common::allowed_fields(audience, &["type"], "audience")?;
            return Ok(Vec::new());
        },
        "users" => ("user_ids", "user"),
        "roles" => ("role_ids", "role"),
        "organizations" => ("organization_ids", "organization"),
        _ => {
            return Err(common::bad(
                "公告接收范围必须是 tenant、users、roles 或 organizations",
            ));
        },
    };
    common::allowed_fields(audience, &["type", field], "audience")?;
    Ok(common::string_list(audience, field, true, 1, 1000, false)?
        .into_iter()
        .map(|id| ResourceReference {
            field: format!("audience.{field}"),
            target_kind: kind.into(),
            target_id: id.into(),
        })
        .collect())
}

#[cfg(test)]
mod tests;
