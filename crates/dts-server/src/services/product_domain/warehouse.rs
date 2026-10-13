mod common;
mod expression;
mod metrics;
mod model;
mod standards;

use crate::models::error::AppError;
use serde_json::{Value, json};

pub use expression::validate as validate_expression;
pub use model::{canonical_data_type, generate_ddl};

pub fn validate(kind: &str, properties: &Value) -> Result<(), AppError> {
    match kind {
        "warehouse_category" | "warehouse_layer" | "warehouse_domain" | "warehouse_process"
        | "warehouse_mart" | "warehouse_subject" => validate_warehouse(kind, properties),
        "naming_standard" | "field_standard" | "enum_standard" | "unit_standard" => {
            standards::validate(kind, properties)
        },
        "data_model" => model::validate(properties),
        "atomic_metric" | "derived_metric" | "composite_metric" | "metric_modifier"
        | "metric_period" => metrics::validate(kind, properties),
        _ => Ok(()),
    }
}

pub fn action(
    kind: &str,
    properties: &Value,
    action: &str,
    payload: &Value,
) -> Result<Option<Value>, AppError> {
    match (kind, action) {
        ("naming_standard", "validate") => standards::validate_name(properties, payload).map(Some),
        ("unit_standard", "convert") => standards::convert_unit(properties, payload).map(Some),
        ("data_model", "validate") => {
            let ddl = generate_ddl(properties)?;
            Ok(Some(json!({
                "valid": true,
                "generated_ddl": ddl,
                "executed": false,
                "validation_scope": "model_definition"
            })))
        },
        ("atomic_metric" | "derived_metric" | "composite_metric", "validate") => {
            validate(kind, properties)?;
            Ok(Some(json!({
                "valid": true,
                "executed": false,
                "validation_scope": "definition_and_expression_syntax",
                "binding_validated": false
            })))
        },
        ("field_standard" | "enum_standard" | "unit_standard" | "metric_period", "validate") => {
            validate(kind, properties)?;
            Ok(Some(
                json!({"valid":true,"executed":false,"validation_scope":"business_definition"}),
            ))
        },
        ("data_model", "publish" | "rollback") => Err(common::bad(
            "模型发布和回滚必须由受控引擎执行服务完成，定义校验不能代替物理操作",
        )),
        ("atomic_metric" | "derived_metric" | "composite_metric", "publish" | "calculate") => Err(
            common::bad("指标发布和计算需要已绑定的数据模型及受控执行服务"),
        ),
        _ => Ok(None),
    }
}

fn validate_warehouse(kind: &str, properties: &Value) -> Result<(), AppError> {
    common::object(properties, "properties")?;
    common::optional_text(properties, "owner_id", 128)?;
    if let Some(sort_order) = properties.get("sort_order") {
        if sort_order
            .as_i64()
            .is_none_or(|value| !(0..=1_000_000).contains(&value))
        {
            return Err(common::bad("sort_order 必须是 0 至 1000000 的整数"));
        }
    }
    match kind {
        "warehouse_layer" => {
            let prefix = common::text(properties, "prefix", 32)?;
            common::identifier(prefix, "prefix")?;
            if !prefix.as_bytes()[0].is_ascii_alphabetic() {
                return Err(common::bad("分层前缀必须以字母开头"));
            }
            common::text(properties, "purpose", 4000)?;
        },
        "warehouse_domain" | "warehouse_mart" => {
            common::text(properties, "category_id", 128)?;
            common::optional_text(properties, "audience", 1000)?;
        },
        "warehouse_process" => {
            common::text(properties, "domain_id", 128)?;
            common::text(properties, "grain", 1000)?;
        },
        "warehouse_subject" => {
            common::text(properties, "mart_id", 128)?;
            common::text(properties, "scope", 4000)?;
        },
        _ => {},
    }
    Ok(())
}

#[cfg(test)]
mod tests;
