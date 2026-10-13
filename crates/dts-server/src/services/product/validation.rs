use std::collections::HashSet;

use serde_json::Value;

use super::catalog::{ProductModule, hierarchical};
use crate::models::error::AppError;

pub fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}

pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}

pub fn strings(value: &Value, field: &str) -> Result<Vec<String>, AppError> {
    let values = value
        .as_array()
        .ok_or_else(|| bad(format!("{field} 必须是数组")))?;
    if values.len() > 500 {
        return Err(bad(format!("{field} 超出数量上限")));
    }
    let mut unique = HashSet::new();
    values
        .iter()
        .map(|v| {
            let text = v
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 256)
                .ok_or_else(|| bad(format!("{field} 必须包含非空文本")))?;
            if !unique.insert(text) {
                return Err(bad(format!("{field} 存在重复值")));
            }
            Ok(text.to_owned())
        })
        .collect()
}

pub fn properties(
    module: &ProductModule,
    value: &Value,
    parent: Option<&str>,
) -> Result<(), AppError> {
    let object = value.as_object().ok_or_else(|| bad("业务属性必须是对象"))?;
    if value.to_string().len() > 256 * 1024 {
        return Err(bad("业务定义超过 256 KiB"));
    }
    if parent.is_some() && !hierarchical(module.resource_kind.as_deref().unwrap_or_default()) {
        return Err(bad("当前业务对象不支持父子层级"));
    }
    for key in object.keys() {
        if !module
            .fields
            .iter()
            .any(|field| &field.key == key && !field.read_only)
        {
            return Err(bad(format!("未知业务字段: {key}")));
        }
        if key == "checkpoint" {
            return Err(bad("消费位点仅由运行引擎提交"));
        }
    }
    for field in &module.fields {
        if field.read_only {
            continue;
        }
        let Some(value) = object.get(&field.key).filter(|value| !value.is_null()) else {
            if field.required {
                return Err(bad(format!("{} 不能为空", field.label.zh)));
            }
            continue;
        };
        let valid = match field.field_type.as_str() {
            "text" | "textarea" => value
                .as_str()
                .is_some_and(|v| v.len() <= 65536 && (!field.required || !v.trim().is_empty())),
            "number" => value.as_f64().is_some_and(f64::is_finite),
            "boolean" => value.is_boolean(),
            "select" => value
                .as_str()
                .is_some_and(|v| field.options.iter().any(|option| option.value == v)),
            "json" => value.is_array() || value.is_object(),
            _ => false,
        };
        if !valid {
            return Err(bad(format!("{} 的类型或范围无效", field.label.zh)));
        }
    }
    reject_secrets(value, 0)?;
    Ok(())
}

fn reject_secrets(value: &Value, depth: usize) -> Result<(), AppError> {
    if depth > 24 {
        return Err(bad("业务配置嵌套过深"));
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let normalized: String = key
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect();
                let sensitive = [
                    "password",
                    "token",
                    "secret",
                    "apikey",
                    "privatekey",
                    "salt",
                ]
                .iter()
                .any(|v| normalized.ends_with(v));
                if sensitive && !value.is_null() {
                    return Err(bad("敏感变量必须使用凭据引用"));
                }
                if key.ends_with("_credential_ref")
                    && value.as_str().is_none_or(|v| {
                        v.strip_prefix("credential:")
                            .is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
                    })
                {
                    return Err(bad("凭据引用格式无效"));
                }
                reject_secrets(value, depth + 1)?;
            }
        },
        Value::Array(values) => {
            for item in values {
                reject_secrets(item, depth + 1)?;
            }
        },
        _ => {},
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn secrets_and_deep_shapes_are_rejected() {
        assert!(reject_secrets(&json!({"dbPassword":"hidden"}), 0).is_err());
        assert!(reject_secrets(&json!({"api_key":"hidden"}), 0).is_err());
        assert!(reject_secrets(&json!({"db_credential_ref":"credential:bad"}), 0).is_err());
        assert!(
            reject_secrets(
                &json!({"db_credential_ref":"credential:47c941eb-3856-4e01-9ee7-cedf7d600131"}),
                0
            )
            .is_ok()
        );
    }
    #[test]
    fn names_and_lists_have_safe_bounds() {
        assert!(identifier("order_item"));
        assert!(!identifier("order; DROP TABLE account"));
        assert!(strings(&json!(["a", "a"]), "fields").is_err());
    }
}
