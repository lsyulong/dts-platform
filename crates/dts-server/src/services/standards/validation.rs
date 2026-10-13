use crate::{models::error::AppError, services::product_domain::warehouse};
use dts_backend::repo::standard_governance::*;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
pub(super) fn identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.len() > 128
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        return Err(bad("标识必须为 1 至 128 字符，不能含首尾空白或控制字符"));
    }
    Ok(())
}
pub(super) fn standard_kind(kind: &str) -> Result<(), AppError> {
    if !STANDARD_KINDS.contains(&kind) {
        return Err(bad("不支持此标准类型"));
    }
    Ok(())
}
pub(super) fn verify_revision(revision: &StandardRevision, tenant: &str) -> Result<(), AppError> {
    standard_kind(&revision.kind)?;
    if revision.tenant_id != tenant
        || revision.state != "active"
        || revision.revision_version < 1
        || revision.definition_hash
            != revision_hash(
                &revision.kind,
                &revision.code,
                &revision.name,
                &revision.properties,
                &revision.children,
            )?
    {
        return Err(AppError::Conflict("标准版本已撤回或证据不一致".into()));
    }
    warehouse::validate(&revision.kind, &revision.properties)?;
    if normalized_standard(&revision.kind, &revision.properties)? != revision.properties {
        return Err(AppError::Conflict(
            "标准修订不是规范化的不可变发布定义".into(),
        ));
    }
    let mut expected_children = Vec::new();
    if revision.kind == "field_standard" {
        for (key, kind) in [("unit", "unit_standard"), ("enum", "enum_standard")] {
            if let Some(id) = revision.properties[format!("{key}_id")].as_str() {
                let version = revision.properties[format!("{key}_version")]
                    .as_i64()
                    .ok_or_else(|| bad("固定子标准版本缺失"))?;
                expected_children.push((kind, id, version));
            }
        }
    }
    if revision.children.len() != expected_children.len()
        || expected_children.iter().any(|(kind, id, version)| {
            !revision.children.iter().any(|child| {
                child.kind == *kind && child.id == *id && child.revision_version == *version
            })
        })
    {
        return Err(AppError::Conflict("标准定义与固定子版本证据不一致".into()));
    }
    Ok(())
}
pub(super) fn normalized_standard(kind: &str, properties: &Value) -> Result<Value, AppError> {
    standard_kind(kind)?;
    let map = properties
        .as_object()
        .ok_or_else(|| bad("标准属性必须为对象"))?;
    let keys: &[&str] = match kind {
        "naming_standard" => &["object_type", "pattern", "example"],
        "field_standard" => &[
            "data_type",
            "nullable",
            "definition",
            "unit_id",
            "unit_version",
            "enum_id",
            "enum_version",
        ],
        "enum_standard" => &["enum_values"],
        "unit_standard" => &["symbol", "dimension", "conversion_factor"],
        _ => return Err(bad("标准类型无效")),
    };
    if map.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(bad("标准含未声明或服务端专属属性"));
    }
    warehouse::validate(kind, properties)?;
    let mut normalized = properties.clone();
    if kind == "field_standard" {
        normalized["data_type"] = Value::String(warehouse::canonical_data_type(
            properties["data_type"].as_str().unwrap_or_default(),
        )?);
        normalized["nullable"] = Value::Bool(properties["nullable"].as_bool().unwrap_or(true));
        for key in ["unit", "enum"] {
            let id_key = format!("{key}_id");
            let version_key = format!("{key}_version");
            match (properties.get(&id_key), properties.get(&version_key)) {
                (None, None) => {},
                (Some(id), Some(version))
                    if id.as_str().is_some() && version.as_i64().is_some_and(|v| v > 0) =>
                {
                    identifier(id.as_str().unwrap_or_default())?;
                },
                _ => return Err(bad("子标准必须同时指定真实 ID 和已发布版本")),
            }
        }
        if properties.get("enum_id").is_some() && normalized["data_type"] != "VARCHAR" {
            return Err(bad(
                "当前字符串枚举内容标准仅可绑定 VARCHAR 字段；其它类型需真实值规则支持",
            ));
        }
        if properties.get("unit_id").is_some() {
            let data_type = normalized["data_type"].as_str().unwrap_or_default();
            if !matches!(
                data_type,
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
            ) && !data_type.starts_with("DECIMAL(")
            {
                return Err(bad("计量单位标准只能绑定数值字段"));
            }
        }
    }
    if kind == "unit_standard" && properties.get("conversion_factor").is_none() {
        normalized["conversion_factor"] = serde_json::json!(1);
    }
    Ok(normalized)
}
pub(super) fn model_properties(properties: &Value) -> Result<Value, AppError> {
    let columns = properties["columns"]
        .as_array()
        .ok_or_else(|| bad("模型字段定义缺失"))?;
    let mut cleaned = properties.clone();
    let mut values = Vec::with_capacity(columns.len());
    let mut names = HashSet::new();
    for column in columns {
        let map = column
            .as_object()
            .ok_or_else(|| bad("模型字段必须为对象"))?;
        if map.keys().any(|k| {
            ![
                "name",
                "data_type",
                "nullable",
                "description",
                "standard_id",
                "standard_version",
                "naming_standard_id",
                "naming_standard_version",
            ]
            .contains(&k.as_str())
        }) {
            return Err(bad("模型字段含未知属性"));
        }
        let name = column["name"]
            .as_str()
            .ok_or_else(|| bad("模型字段名缺失"))?;
        if !names.insert(name.to_owned()) {
            return Err(bad("模型字段名称重复"));
        }
        let mut core = map.clone();
        for key in [
            "standard_id",
            "standard_version",
            "naming_standard_id",
            "naming_standard_version",
        ] {
            core.remove(key);
        }
        values.push(Value::Object(core));
    }
    cleaned["columns"] = Value::Array(values);
    warehouse::generate_ddl(&cleaned)?;
    Ok(cleaned)
}
pub(super) fn effective_nullable(properties: &Value, column: &Value) -> bool {
    let name = column["name"].as_str().unwrap_or_default();
    column["nullable"].as_bool().unwrap_or(true)
        && !properties["primary_key"].as_array().is_some_and(|keys| {
            keys.iter().any(|key| {
                key.as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case(name))
            })
        })
}
pub(super) fn check_field(
    properties: &Value,
    column: &Value,
    standard: &StandardRevision,
) -> Result<(), AppError> {
    let actual = warehouse::canonical_data_type(column["data_type"].as_str().unwrap_or_default())?;
    let required = warehouse::canonical_data_type(
        standard.properties["data_type"]
            .as_str()
            .unwrap_or_default(),
    )?;
    if actual != required {
        return Err(bad("模型字段类型不符合固定字段标准版本"));
    }
    if !standard.properties["nullable"].as_bool().unwrap_or(true)
        && effective_nullable(properties, column)
    {
        return Err(bad("字段标准要求非空，模型字段及主键不能提供该约束"));
    }
    Ok(())
}
pub(super) fn check_name(
    standard: &StandardRevision,
    object_type: &str,
    name: &str,
) -> Result<(), AppError> {
    if standard.kind != "naming_standard" || standard.properties["object_type"] != object_type {
        return Err(bad("命名标准对象类型不匹配"));
    }
    let matched = warehouse::action(
        "naming_standard",
        &standard.properties,
        "validate",
        &serde_json::json!({"sample":name}),
    )?
    .is_some_and(|result| result["matched"] == true);
    if !matched {
        return Err(bad("对象名称不符合固定命名标准版本"));
    }
    Ok(())
}
