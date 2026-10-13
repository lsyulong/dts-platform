use super::{common, expression};
use crate::models::error::AppError;
use serde_json::Value;

pub(super) fn validate(kind: &str, properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    match kind {
        "atomic_metric" => {
            common::text(properties, "process_id", 128)?;
            common::text(properties, "model_id", 128)?;
            common::text(properties, "grain", 1000)?;
            common::optional_text(properties, "unit_id", 128)?;
            expression::validate(common::text(properties, "expression", 16_000)?)?;
        },
        "derived_metric" => {
            common::text(properties, "atomic_metric_id", 128)?;
            common::text(properties, "period_id", 128)?;
            common::optional_text(properties, "modifier_id", 128)?;
            // An empty list deliberately represents a global aggregate with no grouping dimensions.
            for dimension in common::string_list(properties, "dimensions", true, 0, 100, true)? {
                common::identifier(dimension, "dimensions")?;
            }
        },
        "composite_metric" => {
            common::string_list(properties, "metric_ids", true, 1, 100, false)?;
            common::optional_text(properties, "unit_id", 128)?;
            expression::validate(common::text(properties, "expression", 16_000)?)?;
        },
        "metric_modifier" => {
            common::identifier(common::text(properties, "dimension", 128)?, "dimension")?;
            expression::validate(common::text(properties, "condition", 16_000)?)?;
        },
        "metric_period" => validate_period(properties)?,
        _ => {},
    }
    Ok(())
}

fn validate_period(properties: &Value) -> Result<(), AppError> {
    let unit = common::text(properties, "unit", 16)?;
    if !matches!(unit, "day" | "week" | "month" | "year") {
        return Err(common::bad("周期单位必须是 day、week、month 或 year"));
    }
    let length = properties
        .get("length")
        .and_then(Value::as_f64)
        .ok_or_else(|| common::bad("周期长度必须是整数"))?;
    if !length.is_finite() || length.fract() != 0.0 || !(1.0..=10_000.0).contains(&length) {
        return Err(common::bad("周期长度必须是 1 至 10000 的整数"));
    }
    let timezone = common::text(properties, "timezone", 128)?;
    if timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(common::bad(
            "时区必须是有效的 IANA 名称，例如 Asia/Shanghai 或 UTC",
        ));
    }
    Ok(())
}
