// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, types::ApiResponseField};
use crate::{models::error::AppError, services::product_domain::warehouse::canonical_data_type};
use serde_json::Value;

pub(super) fn canonical_type(value: &str) -> Result<String, AppError> {
    let canonical = canonical_data_type(value)?;
    if matches!(
        canonical.as_str(),
        "BOOLEAN"
            | "TINYINT"
            | "SMALLINT"
            | "INTEGER"
            | "BIGINT"
            | "UTINYINT"
            | "USMALLINT"
            | "UINTEGER"
            | "UBIGINT"
            | "FLOAT"
            | "DOUBLE"
            | "VARCHAR"
            | "DATE"
            | "TIMESTAMP"
    ) || canonical.starts_with("DECIMAL(")
    {
        Ok(canonical)
    } else {
        Err(bad(
            "API V1 明细响应尚不支持该复杂或特殊类型，请选择已支持的标量字段",
        ))
    }
}

pub(super) fn declared(
    properties: &Value,
    compiled: Vec<ApiResponseField>,
) -> Result<Vec<ApiResponseField>, AppError> {
    let Some(value) = properties
        .get("response_schema")
        .filter(|value| !value.is_null())
    else {
        return Ok(compiled);
    };
    let mut fields: Vec<ApiResponseField> = serde_json::from_value(value.clone())
        .map_err(|_| bad("响应字段必须为 name/data_type/nullable/description 数组"))?;
    if fields.is_empty() {
        return Ok(compiled);
    }
    if fields.len() != compiled.len() {
        return Err(bad("声明响应字段数量与实际 SQL 投影不一致"));
    }
    for (field, actual) in fields.iter_mut().zip(compiled) {
        field.data_type = canonical_type(&field.data_type)?;
        if field.name != actual.name
            || field.data_type != actual.data_type
            || field.nullable != actual.nullable
        {
            return Err(bad("声明响应字段名、类型或可空性与实际直接字段投影不一致"));
        }
        if field.description.len() > 2048 {
            return Err(bad("响应字段说明超过 2048 字节"));
        }
    }
    Ok(fields)
}

pub(super) fn actual_type(declared: &str, actual: &str) -> bool {
    let actual = actual.replace(' ', "");
    match declared {
        "BOOLEAN" => actual == "Boolean" || actual == "BOOLEAN",
        "TINYINT" => actual == "Int8" || actual == "TINYINT",
        "SMALLINT" => actual == "Int16" || actual == "SMALLINT",
        "INTEGER" => actual == "Int32" || actual == "INTEGER",
        "BIGINT" => actual == "Int64" || actual == "BIGINT",
        "UTINYINT" => actual == "UInt8" || actual == "UTINYINT",
        "USMALLINT" => actual == "UInt16" || actual == "USMALLINT",
        "UINTEGER" => actual == "UInt32" || actual == "UINTEGER",
        "UBIGINT" => actual == "UInt64" || actual == "UBIGINT",
        "FLOAT" => actual == "Float32" || actual == "FLOAT",
        "DOUBLE" => actual == "Float64" || actual == "DOUBLE",
        "VARCHAR" => matches!(
            actual.as_str(),
            "Utf8" | "LargeUtf8" | "Utf8View" | "VARCHAR"
        ),
        "DATE" => matches!(actual.as_str(), "Date32" | "Date64" | "DATE"),
        "TIMESTAMP" => [
            "TIMESTAMP",
            "Timestamp(Second,None)",
            "Timestamp(Millisecond,None)",
            "Timestamp(Microsecond,None)",
            "Timestamp(Nanosecond,None)",
        ]
        .contains(&actual.as_str()),
        _ if declared.starts_with("DECIMAL(") => {
            let suffix = declared.strip_prefix("DECIMAL").unwrap_or_default();
            actual == declared
                || ["Decimal32", "Decimal64", "Decimal128", "Decimal256"]
                    .iter()
                    .any(|prefix| actual.strip_prefix(prefix) == Some(suffix))
        },
        _ => false,
    }
}

pub(super) fn actual_value(field: &ApiResponseField, value: &Value) -> bool {
    if value.is_null() {
        return field.nullable;
    }
    match field.data_type.as_str() {
        "VARCHAR" => value.is_string(),
        "BOOLEAN" => value.is_boolean(),
        "TINYINT" | "SMALLINT" | "INTEGER" | "BIGINT" => {
            signed(value).is_some_and(|value| match field.data_type.as_str() {
                "TINYINT" => i8::try_from(value).is_ok(),
                "SMALLINT" => i16::try_from(value).is_ok(),
                "INTEGER" => i32::try_from(value).is_ok(),
                _ => true,
            })
        },
        "UTINYINT" | "USMALLINT" | "UINTEGER" | "UBIGINT" => {
            unsigned(value).is_some_and(|value| match field.data_type.as_str() {
                "UTINYINT" => u8::try_from(value).is_ok(),
                "USMALLINT" => u16::try_from(value).is_ok(),
                "UINTEGER" => u32::try_from(value).is_ok(),
                _ => true,
            })
        },
        "FLOAT" | "DOUBLE" => value.as_f64().is_some_and(f64::is_finite),
        "DATE" => value
            .as_str()
            .is_some_and(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()),
        "TIMESTAMP" => value.as_str().is_some_and(|value| {
            ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
                .iter()
                .any(|format| chrono::NaiveDateTime::parse_from_str(value, format).is_ok())
        }),
        data_type if data_type.starts_with("DECIMAL(") => value
            .as_str()
            .is_some_and(|value| decimal_text(data_type, value)),
        _ => false,
    }
}

fn signed(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

fn unsigned(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn decimal_text(data_type: &str, value: &str) -> bool {
    let Some((precision, scale)) = data_type
        .strip_prefix("DECIMAL(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.split_once(','))
        .and_then(|(p, s)| Some((p.parse::<usize>().ok()?, s.parse::<usize>().ok()?)))
    else {
        return false;
    };
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    !integer.is_empty()
        && integer.bytes().all(|c| c.is_ascii_digit())
        && integer.trim_start_matches('0').len() <= precision.saturating_sub(scale)
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= scale
                && fraction.bytes().all(|c| c.is_ascii_digit())
        })
        && parts.next().is_none()
}
