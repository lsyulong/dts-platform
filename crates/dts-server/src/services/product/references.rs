use dts_backend::ResourceReference;
use serde_json::Value;

use super::{
    catalog::ProductModule,
    validation::{bad, strings},
};
use crate::models::error::AppError;

pub fn extract(
    module: &ProductModule,
    properties: &Value,
) -> Result<Vec<ResourceReference>, AppError> {
    let mut result = Vec::new();
    if module.resource_kind.as_deref() == Some("announcement") {
        result.extend(crate::services::product_domain::access::audience_references(properties)?);
    }
    for field in &module.fields {
        let Some(value) = properties.get(&field.key).filter(|v| !v.is_null()) else {
            continue;
        };
        if let Some(kind) = &field.reference_kind {
            let id = value
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("业务引用必须是对象 ID"))?;
            result.push(reference(&field.key, kind, id));
        }
        let array_kind = match field.key.as_str() {
            "member_ids" | "recipient_ids" => Some("user"),
            "authorized_role_ids" => Some("role"),
            "menu_ids" => Some("system_menu"),
            "rule_ids" => Some("quality_rule"),
            "asset_ids" => Some("asset_entry"),
            _ => None,
        };
        if let Some(kind) = array_kind {
            for id in strings(value, &field.key)? {
                result.push(reference(&field.key, kind, &id));
            }
        }
        if matches!(field.key.as_str(), "manager_user_id" | "assignee_id") {
            let id = value
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("请选择实际用户"))?;
            result.push(reference(&field.key, "user", id));
        }
    }
    if module.resource_kind.as_deref() == Some("data_model") {
        if let Some(columns) = properties["columns"].as_array() {
            for (index, column) in columns.iter().enumerate() {
                for (key, kind) in [
                    ("standard_id", "field_standard"),
                    ("naming_standard_id", "naming_standard"),
                ] {
                    if let Some(id) = column[key].as_str() {
                        result.push(reference(&format!("columns.{index}.{key}"), kind, id));
                    }
                }
            }
        }
    }
    if module.resource_kind.as_deref() == Some("development_config") {
        let policy = dts_backend::repo::runtime_configuration::validation::policy(properties)?;
        if let (Some(kind), Some(id)) = (policy.source_kind, policy.source_id) {
            result.push(reference("source_id", &kind, &id));
        }
    }
    Ok(result)
}

fn reference(field: &str, kind: &str, id: &str) -> ResourceReference {
    ResourceReference {
        field: field.into(),
        target_kind: kind.into(),
        target_id: id.into(),
    }
}
