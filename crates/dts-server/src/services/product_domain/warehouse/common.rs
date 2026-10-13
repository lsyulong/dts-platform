use crate::models::error::AppError;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub(super) fn bad(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}

pub(super) fn standard_pin(value: &Value, id_key: &str, version_key: &str) -> Result<(), AppError> {
    match (value.get(id_key), value.get(version_key)) {
        (None, None) => Ok(()),
        (Some(Value::String(id)), Some(version))
            if !id.is_empty()
                && id.len() <= 128
                && !id.chars().any(char::is_control)
                && version.as_i64().is_some_and(|v| v > 0) =>
        {
            Ok(())
        },
        _ => Err(bad(format!(
            "{id_key} 与 {version_key} 必须成对指定真实标准 ID 和正整数发布版本"
        ))),
    }
}

pub(super) fn object<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a Map<String, Value>, AppError> {
    value
        .as_object()
        .ok_or_else(|| bad(format!("{field} 必须是对象")))
}

pub(super) fn text<'a>(value: &'a Value, field: &str, max: usize) -> Result<&'a str, AppError> {
    let text = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{field} 必须是非空字符串")))?;
    check_text(text, field, max)?;
    Ok(text)
}

pub(super) fn optional_text<'a>(
    value: &'a Value,
    field: &str,
    max: usize,
) -> Result<Option<&'a str>, AppError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.is_empty() => Ok(None),
        Some(Value::String(text)) => {
            check_text(text, field, max)?;
            Ok(Some(text))
        },
        _ => Err(bad(format!("{field} 必须是字符串"))),
    }
}

pub(super) fn check_text(text: &str, field: &str, max: usize) -> Result<(), AppError> {
    if text.trim().is_empty()
        || text.chars().count() > max
        || text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(bad(format!("{field} 必须是 1 至 {max} 字符的有效文本")));
    }
    Ok(())
}

pub(super) fn boolean(value: &Value, field: &str, default: bool) -> Result<bool, AppError> {
    match value.get(field) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(bad(format!("{field} 必须是布尔值"))),
    }
}

pub(super) fn identifier(value: &str, field: &str) -> Result<(), AppError> {
    let mut bytes = value.bytes();
    if value.len() > 128
        || bytes
            .next()
            .is_none_or(|first| !first.is_ascii_alphabetic() && first != b'_')
        || bytes.any(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
    {
        return Err(bad(format!(
            "{field} 只能使用字母、数字和下划线，且不能以数字开头"
        )));
    }
    Ok(())
}

pub(super) fn string_list<'a>(
    value: &'a Value,
    field: &str,
    required: bool,
    min: usize,
    max: usize,
    ignore_case: bool,
) -> Result<Vec<&'a str>, AppError> {
    let Some(value) = value.get(field) else {
        return if required {
            Err(bad(format!("缺少 {field}")))
        } else {
            Ok(Vec::new())
        };
    };
    let values = value
        .as_array()
        .ok_or_else(|| bad(format!("{field} 必须是字符串数组")))?;
    if values.len() < min || values.len() > max {
        return Err(bad(format!("{field} 项数必须在 {min} 至 {max} 之间")));
    }
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let text = value
            .as_str()
            .ok_or_else(|| bad(format!("{field} 必须是字符串数组")))?;
        check_text(text, field, 128)?;
        if text != text.trim() {
            return Err(bad(format!("{field} 不能含有首尾空白")));
        }
        let key = if ignore_case {
            text.to_ascii_lowercase()
        } else {
            text.to_owned()
        };
        if !seen.insert(key) {
            return Err(bad(format!("{field} 不能包含重复项")));
        }
        result.push(text);
    }
    Ok(result)
}

pub(super) fn allowed_fields(value: &Value, fields: &[&str], label: &str) -> Result<(), AppError> {
    if object(value, label)?
        .keys()
        .any(|key| !fields.contains(&key.as_str()))
    {
        return Err(bad(format!("{label} 包含不支持的字段")));
    }
    Ok(())
}
