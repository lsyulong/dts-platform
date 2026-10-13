use super::{common, model};
use crate::models::error::AppError;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn validate(kind: &str, properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    match kind {
        "naming_standard" => {
            let object_type = common::text(properties, "object_type", 32)?;
            if !matches!(object_type, "table" | "column" | "metric") {
                return Err(common::bad("object_type 必须是 table、column 或 metric"));
            }
            let regex = naming_regex(properties)?;
            if let Some(example) = common::optional_text(properties, "example", 256)? {
                if !regex.is_match(example) {
                    return Err(common::bad("命名示例不符合当前完整匹配的命名规则"));
                }
            }
        },
        "field_standard" => {
            model::canonical_data_type(common::text(properties, "data_type", 64)?)?;
            common::boolean(properties, "nullable", true)?;
            common::text(properties, "definition", 4000)?;
            common::standard_pin(properties, "unit_id", "unit_version")?;
            common::standard_pin(properties, "enum_id", "enum_version")?;
        },
        "enum_standard" => validate_enumeration(properties)?,
        "unit_standard" => {
            unit(properties)?;
        },
        _ => {},
    }
    Ok(())
}

fn naming_regex(properties: &Value) -> Result<Regex, AppError> {
    let pattern = common::text(properties, "pattern", 2048)?;
    RegexBuilder::new(&format!("\\A(?:{pattern})\\z"))
        .size_limit(1_000_000)
        .dfa_size_limit(1_000_000)
        .build()
        .map_err(|_| common::bad("命名规则无效或过于复杂；请使用受支持的正则表达式语法"))
}

pub(super) fn validate_name(properties: &Value, payload: &Value) -> Result<Value, AppError> {
    validate("naming_standard", properties)?;
    common::object(payload, "payload")?;
    let sample_key = if payload.get("sample").is_some() {
        "sample"
    } else {
        "name"
    };
    let sample = common::text(payload, sample_key, 256)?;
    let matched = naming_regex(properties)?.is_match(sample);
    Ok(json!({
        "valid": matched,
        "matched": matched,
        "sample": sample,
        "validation_scope": "full_name_match"
    }))
}

fn validate_enumeration(properties: &Value) -> Result<(), AppError> {
    let values = properties
        .get("enum_values")
        .and_then(Value::as_array)
        .ok_or_else(|| common::bad("enum_values 必须是枚举对象数组"))?;
    if values.is_empty() || values.len() > 1000 {
        return Err(common::bad("枚举必须包含 1 至 1000 项"));
    }
    let mut seen = HashSet::new();
    for value in values {
        common::allowed_fields(
            value,
            &["value", "label", "description"],
            "enum_values item",
        )?;
        let code = common::text(value, "value", 128)?;
        if code != code.trim() || !seen.insert(code) {
            return Err(common::bad("枚举编码不能重复或包含首尾空白"));
        }
        common::text(value, "label", 256)?;
        common::optional_text(value, "description", 2000)?;
    }
    Ok(())
}

struct Unit<'a> {
    symbol: &'a str,
    dimension: &'a str,
    factor: f64,
}

fn unit(properties: &Value) -> Result<Unit<'_>, AppError> {
    common::object(properties, "unit")?;
    let symbol = common::text(properties, "symbol", 64)?;
    let dimension = common::text(properties, "dimension", 128)?;
    if dimension != dimension.trim() {
        return Err(common::bad("计量维度不能含有首尾空白"));
    }
    let factor = match properties.get("conversion_factor") {
        None => 1.0,
        Some(value) => value
            .as_f64()
            .ok_or_else(|| common::bad("换算系数必须是数字"))?,
    };
    if !factor.is_finite() || factor <= 0.0 {
        return Err(common::bad(
            "换算系数必须是正有限数，基准单位可省略并使用 1",
        ));
    }
    Ok(Unit {
        symbol,
        dimension,
        factor,
    })
}

// The caller must supply a target hydrated from an authorized same-tenant unit ID.
// This pure helper cannot establish tenant membership or access to a target resource.
pub(super) fn convert_unit(properties: &Value, payload: &Value) -> Result<Value, AppError> {
    let source = unit(properties)?;
    common::object(payload, "payload")?;
    let value = payload
        .get("value")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| common::bad("value 必须是有限数字"))?;
    let target = unit(
        payload
            .get("target")
            .ok_or_else(|| common::bad("缺少已授权的目标单位"))?,
    )?;
    if source.dimension != target.dimension {
        return Err(common::bad("只支持同一计量维度的单位换算"));
    }
    let converted = if value == 0.0 {
        0.0
    } else {
        // Alternative operation orders avoid overflowing an intermediate product or ratio.
        [
            value * (source.factor / target.factor),
            (value / target.factor) * source.factor,
            (value * source.factor) / target.factor,
        ]
        .into_iter()
        .find(|result| result.is_finite() && *result != 0.0)
        .ok_or_else(|| common::bad("换算结果超出可表示的有限数范围"))?
    };
    Ok(json!({
        "value": value,
        "converted_value": converted,
        "source_symbol": source.symbol,
        "target_symbol": target.symbol,
        "dimension": source.dimension,
        "conversion_mode": "multiplicative"
    }))
}
