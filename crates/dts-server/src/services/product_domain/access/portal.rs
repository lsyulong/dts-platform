use super::common;
use crate::{models::error::AppError, services::product::catalog};
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn validate(properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::string_list(properties, "authorized_role_ids", true, 1, 1000, false)?;
    let layout = properties
        .get("layout")
        .ok_or_else(|| common::bad("缺少门户布局"))?;
    common::allowed_fields(layout, &["columns", "widgets"], "layout")?;
    if layout["columns"].as_i64() != Some(12) {
        return Err(common::bad("门户布局必须使用 12 列栅格"));
    }
    let widgets = layout["widgets"]
        .as_array()
        .filter(|widgets| widgets.len() <= 48)
        .ok_or_else(|| common::bad("门户组件必须是数组且最多包含 48 项"))?;
    let modules = catalog::modules()?;
    let mut ids = HashSet::new();
    for widget in widgets {
        common::allowed_fields(
            widget,
            &["id", "type", "title", "module_id", "width", "height"],
            "widget",
        )?;
        let id = common::text(widget, "id", 80)?;
        if id != id.trim() || !ids.insert(id) {
            return Err(common::bad("门户组件标识必须唯一且没有首尾空白"));
        }
        if !matches!(
            common::text(widget, "type", 32)?,
            "metric" | "task_list" | "datasource_list" | "announcement" | "chart" | "shortcut"
        ) {
            return Err(common::bad("门户组件类型不受支持"));
        }
        common::text(widget, "title", 120)?;
        let module_id = common::text(widget, "module_id", 128)?;
        if !modules.iter().any(|module| module.id == module_id) {
            return Err(common::bad("组件必须关联实际存在的产品模块"));
        }
        for (key, max) in [("width", 12), ("height", 8)] {
            if widget[key]
                .as_i64()
                .is_none_or(|value| !(1..=max).contains(&value))
            {
                return Err(common::bad("组件尺寸必须是合法的整数栅格大小"));
            }
        }
    }
    if let Some(theme) = properties.get("theme").filter(|value| !value.is_null()) {
        common::allowed_fields(
            theme,
            &["primary_color", "background_color", "title"],
            "theme",
        )?;
        common::text(theme, "title", 120)?;
        for key in ["primary_color", "background_color"] {
            let color = common::text(theme, key, 7)?;
            if color.len() != 7
                || !color.starts_with('#')
                || !color.bytes().skip(1).all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(common::bad("门户颜色必须是 #RRGGBB 格式"));
            }
        }
    }
    Ok(())
}
