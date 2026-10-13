// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::BackendError;

fn bad(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}

pub fn text<'a>(value: &'a Value, field: &str, limit: usize) -> Result<&'a str, BackendError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && text.len() <= limit && !text.contains('\0'))
        .ok_or_else(|| bad(&format!("{field} 不能为空且不能超过 {limit} 字节")))
}

pub fn schema(properties: &Value) -> Result<(), BackendError> {
    if serde_json::to_string(properties)
        .map_err(|e| BackendError::Internal(e.to_string()))?
        .len()
        > 256 * 1024
    {
        return Err(bad("主数据模型定义超过256KiB"));
    }
    let columns = properties
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("columns 必须是字段数组"))?;
    if columns.is_empty() || columns.len() > 256 {
        return Err(bad("主数据模型必须有 1 至 256 个字段"));
    }
    let mut names = HashSet::new();
    for column in columns {
        let name = text(column, "name", 128)?;
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "password"
                | "pwd"
                | "password_hash"
                | "secret"
                | "token"
                | "access_token"
                | "refresh_token"
                | "api_key"
                | "private_key"
                | "client_secret"
        ) {
            return Err(bad("主数据模型不能存储运行凭据字段，请使用独立凭据设施"));
        }
        if !name
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !names.insert(name.to_ascii_lowercase())
        {
            return Err(bad("字段名必须是唯一合法标识符（不区分大小写）"));
        }
        data_type(text(column, "data_type", 64)?)?;
        if column
            .get("nullable")
            .is_none_or(|value| !value.is_boolean())
        {
            return Err(bad("字段 nullable 必须是布尔值"));
        }
        if column
            .get("description")
            .is_some_and(|value| value.as_str().is_none_or(|text| text.len() > 2000))
        {
            return Err(bad("字段说明必须是最多2000字节的文本"));
        }
        if column.as_object().is_none_or(|value| {
            value.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "name" | "data_type" | "nullable" | "description"
                )
            })
        }) {
            return Err(bad("模型字段包含未支持属性"));
        }
    }
    let keys = properties
        .get("key_fields")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("key_fields 必须为字段数组"))?;
    if keys.is_empty() || keys.len() > columns.len() {
        return Err(bad("唯一标识字段不能为空"));
    }
    let mut unique = HashSet::new();
    for key in keys {
        let key = key
            .as_str()
            .ok_or_else(|| bad("key_fields 只能包含字段名"))?;
        if !unique.insert(key) {
            return Err(bad("唯一标识字段不能重复"));
        }
        let column = columns
            .iter()
            .find(|column| column["name"].as_str() == Some(key))
            .ok_or_else(|| bad("唯一标识字段必须存在于模型"))?;
        let kind = data_type(text(column, "data_type", 64)?)?;
        if column["nullable"] != Value::Bool(false) {
            return Err(bad("唯一标识字段必须设置为不允许空值"));
        }
        if matches!(kind.as_str(), "JSON" | "DOUBLE" | "FLOAT") {
            return Err(bad("唯一标识不能使用 JSON 或浮点数"));
        }
    }
    Ok(())
}

pub fn data_type(raw: &str) -> Result<String, BackendError> {
    let value = raw.trim().to_ascii_uppercase().replace(' ', "");
    let simple = match value.as_str() {
        "TEXT" | "STRING" | "VARCHAR" => Some("VARCHAR"),
        "BOOL" | "BOOLEAN" => Some("BOOLEAN"),
        "INT" | "INTEGER" | "INT32" => Some("INTEGER"),
        "BIGINT" | "INT64" => Some("BIGINT"),
        "SMALLINT" | "INT16" => Some("SMALLINT"),
        "TINYINT" | "INT8" => Some("TINYINT"),
        "REAL" | "FLOAT" => Some("FLOAT"),
        "DOUBLE" => Some("DOUBLE"),
        "DATE" => Some("DATE"),
        "TIMESTAMP" | "TIMESTAMPTZ" => Some("TIMESTAMP"),
        "UUID" => Some("UUID"),
        "JSON" => Some("JSON"),
        _ => None,
    };
    if let Some(value) = simple {
        return Ok(value.into());
    }
    let decimal = value
        .strip_prefix("DECIMAL(")
        .or_else(|| value.strip_prefix("NUMERIC("))
        .and_then(|text| text.strip_suffix(')'));
    if let Some(decimal) = decimal {
        let mut parts = decimal.split(',');
        if let (Some(p), Some(s)) = (
            parts.next().and_then(|p| p.parse::<u8>().ok()),
            parts.next().and_then(|s| s.parse::<u8>().ok()),
        ) {
            if parts.next().is_none() && (1..=38).contains(&p) && s <= p {
                return Ok(format!("DECIMAL({p},{s})"));
            }
        }
    }
    Err(bad(
        "主数据仅支持标量、JSON 和精确 DECIMAL(p,s) 类型，不接受未知类型",
    ))
}

pub fn record(schema_value: &Value, properties: &Value) -> Result<String, BackendError> {
    schema(schema_value)?;
    text(properties, "model_id", 128)?;
    text(properties, "business_key", 1024)?;
    let values = properties
        .get("values")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("values 必须为字段值对象"))?;
    if serde_json::to_string(values)
        .map_err(|e| BackendError::Internal(e.to_string()))?
        .len()
        > 256 * 1024
    {
        return Err(bad("主数据字段值超过 256KiB"));
    }
    let columns = schema_value["columns"]
        .as_array()
        .ok_or_else(|| bad("模型字段缺失"))?;
    if values.keys().any(|name| {
        !columns
            .iter()
            .any(|column| column["name"].as_str() == Some(name))
    }) {
        return Err(bad("记录包含模型未定义字段"));
    }
    let mut canonical = serde_json::Map::new();
    for column in columns {
        let name = text(column, "name", 128)?;
        let value = values.get(name).unwrap_or(&Value::Null);
        if value.is_null() {
            if column["nullable"] != Value::Bool(true) {
                return Err(bad(&format!("{name} 不允许空值")));
            }
            canonical.insert(name.into(), Value::Null);
        } else {
            canonical.insert(
                name.into(),
                scalar(&data_type(text(column, "data_type", 64)?)?, value)?,
            );
        }
    }
    let keys = schema_value["key_fields"]
        .as_array()
        .ok_or_else(|| bad("模型标识缺失"))?;
    let identity = keys
        .iter()
        .map(|key| {
            let name = key.as_str().ok_or_else(|| bad("标识字段错误"))?;
            let value = canonical.get(name).ok_or_else(|| bad("唯一标识字段缺失"))?;
            if value.is_null() || value.as_str().is_some_and(|value| value.trim().is_empty()) {
                return Err(bad("唯一标识不能为 null 或空文本"));
            }
            Ok(json!([name, value]))
        })
        .collect::<Result<Vec<Value>, BackendError>>()?;
    let canonical =
        serde_json::to_string(&identity).map_err(|e| BackendError::Internal(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}

fn scalar(kind: &str, value: &Value) -> Result<Value, BackendError> {
    let result = match kind {
        "VARCHAR" => value
            .as_str()
            .filter(|text| text.len() <= 64 * 1024)
            .map(|text| json!(text)),
        "BOOLEAN" => value.as_bool().map(|value| json!(value)),
        "INTEGER" | "BIGINT" | "SMALLINT" | "TINYINT" => value
            .as_i64()
            .filter(|value| match kind {
                "INTEGER" => i32::try_from(*value).is_ok(),
                "SMALLINT" => i16::try_from(*value).is_ok(),
                "TINYINT" => i8::try_from(*value).is_ok(),
                _ => true,
            })
            .map(|value| json!(value)),
        "FLOAT" | "DOUBLE" => value
            .as_f64()
            .filter(|value| value.is_finite() && (kind == "DOUBLE" || (*value as f32).is_finite()))
            .map(|value| json!(value)),
        "DATE" => value
            .as_str()
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .map(|value| json!(value.to_string())),
        "TIMESTAMP" => value
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| {
                json!(
                    value
                        .with_timezone(&chrono::Utc)
                        .format("%Y-%m-%dT%H:%M:%S%.9fZ")
                        .to_string()
                )
            }),
        "UUID" => value
            .as_str()
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .map(|value| json!(value.to_string())),
        "JSON" => Some(value.clone()),
        _ if kind.starts_with("DECIMAL(") => decimal(kind, value),
        _ => None,
    };
    result.ok_or_else(|| {
        bad(&format!(
            "字段值不符合 {kind} 类型，decimal 必须为精确十进制字符串"
        ))
    })
}

fn decimal(kind: &str, value: &Value) -> Option<Value> {
    let mut parts = kind.strip_prefix("DECIMAL(")?.strip_suffix(')')?.split(',');
    let precision = parts.next()?.parse::<usize>().ok()?;
    let scale = parts.next()?.parse::<usize>().ok()?;
    let raw = value.as_str()?;
    let (negative, digits) = raw
        .strip_prefix('-')
        .map_or((false, raw), |digits| (true, digits));
    let mut parts = digits.split('.');
    let integer = parts.next()?;
    let fractional = parts.next().unwrap_or("");
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|c| c.is_ascii_digit())
        || !fractional.bytes().all(|c| c.is_ascii_digit())
        || fractional.len() > scale
    {
        return None;
    }
    let integer = integer.trim_start_matches('0');
    if integer.len() > precision - scale {
        return None;
    }
    let integer = if integer.is_empty() { "0" } else { integer };
    let fractional = fractional.trim_end_matches('0');
    let negative = if negative && (integer != "0" || !fractional.is_empty()) {
        "-"
    } else {
        ""
    };
    Some(json!(if fractional.is_empty() {
        format!("{negative}{integer}")
    } else {
        format!("{negative}{integer}.{fractional}")
    }))
}

pub fn rule(properties: &Value) -> Result<(String, u32, i64, i64), BackendError> {
    text(properties, "model_id", 128)?;
    let prefix = properties
        .get("prefix")
        .and_then(Value::as_str)
        .unwrap_or("");
    if prefix.len() > 32
        || !prefix
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(bad("编码前缀最多32位字母、数字、下划线或短横线"));
    }
    let width = properties
        .get("width")
        .and_then(Value::as_u64)
        .filter(|width| (1..=18).contains(width))
        .ok_or_else(|| bad("序号宽度必须为1至18"))? as u32;
    let maximum = 10_i64.pow(width) - 1;
    let start = properties
        .get("start_value")
        .and_then(Value::as_i64)
        .filter(|value| (0..=maximum).contains(value))
        .ok_or_else(|| bad("起始序号超出编码位宽"))?;
    Ok((prefix.into(), width, start, maximum))
}
