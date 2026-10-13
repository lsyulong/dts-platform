use super::{bad, types::*};
use crate::{models::error::AppError, services::product_domain::warehouse::canonical_data_type};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn parse<T: DeserializeOwned>(properties: &Value) -> Result<T, AppError> {
    serde_json::from_value(definition_properties(properties)?)
        .map_err(|_| bad("指标字段缺失、未知或类型不符合受控定义"))
}
pub(super) fn definition_properties(properties: &Value) -> Result<Value, AppError> {
    let mut properties = properties
        .as_object()
        .cloned()
        .ok_or_else(|| bad("指标定义必须是对象"))?;
    properties.remove("publication");
    Ok(Value::Object(properties))
}
pub(super) fn text(value: &str, max: usize) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > max
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(bad("指标文字或标识无效"));
    }
    Ok(())
}
pub(super) fn quote(value: &str) -> Result<String, AppError> {
    text(value, 256)?;
    Ok(format!("\"{}\"", value.replace('"', "\"\"")))
}
pub(super) fn ids(values: &[String], min: usize, max: usize) -> Result<(), AppError> {
    if !(min..=max).contains(&values.len()) {
        return Err(bad("指标字段或依赖数量超出范围"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        text(value, 128)?;
        if !seen.insert(value.to_lowercase()) {
            return Err(bad("指标字段或依赖不能重复"));
        }
    }
    Ok(())
}
pub(super) fn numeric(value: &str) -> bool {
    canonical_data_type(value).is_ok_and(|value| {
        matches!(
            value.as_str(),
            "TINYINT"
                | "SMALLINT"
                | "INTEGER"
                | "BIGINT"
                | "HUGEINT"
                | "UTINYINT"
                | "USMALLINT"
                | "UINTEGER"
                | "UBIGINT"
                | "UHUGEINT"
                | "FLOAT"
                | "DOUBLE"
        ) || value.starts_with("DECIMAL(")
    })
}
pub(super) fn unit(properties: &Value) -> Result<UnitDefinition, AppError> {
    let unit: UnitDefinition = parse(properties)?;
    text(&unit.symbol, 128)?;
    text(&unit.dimension, 128)?;
    if !unit.conversion_factor.is_finite() || unit.conversion_factor <= 0.0 {
        return Err(bad("单位基准换算系数必须为正有限值"));
    }
    Ok(unit)
}
pub fn validate_definition(kind: &str, properties: &Value) -> Result<(), AppError> {
    match kind {
        "atomic_metric" => {
            let value: AtomicDefinition = parse(properties)?;
            super::standard_dependencies::validate_unit_pair(
                value.unit_id.as_deref(),
                value.unit_version,
            )?;
            for id in [
                &value.process_id,
                &value.model_id,
                &value.metadata_object_id,
                &value.time_column,
            ] {
                text(id, 128)?;
            }
            text(&value.grain, 1000)?;
            text(&value.expression, 16000)?;
            ids(&value.grain_columns, 1, 32)?;
            ids(&value.dimensions, 0, 32)?;
            if let Some(unit) = value.unit_id {
                text(&unit, 128)?;
            }
            value
                .time_timezone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| bad("时间字段时区必须是有效IANA时区"))?;
            super::expression::aggregate(&value.expression, None)?;
        },
        "derived_metric" => {
            let value: DerivedDefinition = parse(properties)?;
            text(&value.atomic_metric_id, 128)?;
            text(&value.period_id, 128)?;
            ids(&value.dimensions, 0, 32)?;
            if let Some(id) = value.modifier_id {
                text(&id, 128)?;
            }
        },
        "composite_metric" => {
            let value: CompositeDefinition = parse(properties)?;
            super::standard_dependencies::validate_unit_pair(
                value.unit_id.as_deref(),
                value.unit_version,
            )?;
            ids(&value.metric_ids, 1, 32)?;
            text(&value.expression, 16000)?;
            if let Some(id) = value.unit_id {
                text(&id, 128)?;
            }
            super::expression::formula(&value.expression, &[])?;
        },
        "metric_modifier" => {
            let value: ModifierDefinition = parse(properties)?;
            text(&value.metadata_object_id, 128)?;
            text(&value.dimension, 128)?;
            text(&value.condition, 16000)?;
            super::predicate::validate_syntax(&value.condition)?;
        },
        "metric_period" => {
            let value: PeriodDefinition = parse(properties)?;
            super::window::validate_period(&value)?;
        },
        _ => return Err(bad("未知指标实体类型")),
    }
    Ok(())
}
