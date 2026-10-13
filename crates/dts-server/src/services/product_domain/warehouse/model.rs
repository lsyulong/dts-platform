use super::common;
use crate::models::error::AppError;
use serde_json::Value;
use std::collections::HashSet;

struct Column {
    name: String,
    data_type: String,
    nullable: bool,
}

struct Model {
    table_name: String,
    columns: Vec<Column>,
    primary_key: Vec<String>,
}

pub(super) fn validate(properties: &Value) -> Result<(), AppError> {
    parse_model(properties).map(|_| ())
}

/// Generates one CREATE TABLE statement exclusively from validated typed fields.
/// Tenant engine routing, authorization and physical execution belong to the writer.
pub fn generate_ddl(properties: &Value) -> Result<String, AppError> {
    let model = parse_model(properties)?;
    let mut definitions = Vec::with_capacity(model.columns.len() + 1);
    for column in &model.columns {
        let primary = model
            .primary_key
            .iter()
            .any(|key| key.eq_ignore_ascii_case(&column.name));
        let required = if !column.nullable || primary {
            " NOT NULL"
        } else {
            ""
        };
        definitions.push(format!(
            "  \"{}\" {}{required}",
            column.name, column.data_type
        ));
    }
    if !model.primary_key.is_empty() {
        let keys = model
            .primary_key
            .iter()
            .map(|key| format!("\"{key}\""))
            .collect::<Vec<_>>()
            .join(", ");
        definitions.push(format!("  PRIMARY KEY ({keys})"));
    }
    let table = model
        .table_name
        .split('.')
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(".");
    Ok(format!(
        "CREATE TABLE {table} (\n{}\n);",
        definitions.join(",\n")
    ))
}

fn parse_model(properties: &Value) -> Result<Model, AppError> {
    common::object(properties, "properties")?;
    common::text(properties, "layer_id", 128)?;
    common::optional_text(properties, "domain_id", 128)?;
    common::standard_pin(properties, "naming_standard_id", "naming_standard_version")?;
    if properties
        .get("standard_coverage")
        .is_some_and(|coverage| !matches!(coverage.as_str(), Some("bound_fields" | "all_fields")))
    {
        return Err(common::bad("standard_coverage 必须来自实际标准绑定事务"));
    }
    let table_name = common::text(properties, "table_name", 257)?;
    let parts: Vec<_> = table_name.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(common::bad(
            "table_name 必须是表名或 schema.table，不能跨数据库命名",
        ));
    }
    for part in parts {
        common::identifier(part, "table_name")?;
    }
    let values = properties
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| common::bad("columns 必须是字段定义数组"))?;
    if values.is_empty() || values.len() > 1000 {
        return Err(common::bad("模型必须包含 1 至 1000 个字段"));
    }
    let mut names = HashSet::new();
    let mut columns = Vec::with_capacity(values.len());
    for value in values {
        common::allowed_fields(
            value,
            &[
                "name",
                "data_type",
                "nullable",
                "description",
                "standard_id",
                "standard_version",
                "naming_standard_id",
                "naming_standard_version",
            ],
            "column",
        )?;
        let name = common::text(value, "name", 128)?;
        common::identifier(name, "column.name")?;
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(common::bad("模型字段名不能重复（不区分大小写）"));
        }
        let data_type = canonical_data_type(common::text(value, "data_type", 64)?)?;
        let nullable = common::boolean(value, "nullable", true)?;
        common::optional_text(value, "description", 2000)?;
        common::standard_pin(value, "standard_id", "standard_version")?;
        common::standard_pin(value, "naming_standard_id", "naming_standard_version")?;
        columns.push(Column {
            name: name.to_owned(),
            data_type,
            nullable,
        });
    }
    let keys = common::string_list(properties, "primary_key", false, 0, 1000, true)?;
    let mut primary_key = Vec::with_capacity(keys.len());
    for key in keys {
        common::identifier(key, "primary_key")?;
        let column = columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(key))
            .ok_or_else(|| common::bad("主键必须引用模型中已经定义的字段"))?;
        if !primary_key_type(&column.data_type) {
            return Err(common::bad(
                "主键类型必须是标量可比较类型，不能使用 JSON 或 BLOB",
            ));
        }
        primary_key.push(column.name.clone());
    }
    Ok(Model {
        table_name: table_name.to_owned(),
        columns,
        primary_key,
    })
}

pub fn canonical_data_type(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_uppercase();
    let simple = match normalized.as_str() {
        "BOOL" | "BOOLEAN" => "BOOLEAN",
        "TINYINT" => "TINYINT",
        "SMALLINT" | "INT2" => "SMALLINT",
        "INT" | "INTEGER" | "INT4" => "INTEGER",
        "BIGINT" | "INT8" => "BIGINT",
        "HUGEINT" => "HUGEINT",
        "UTINYINT" => "UTINYINT",
        "USMALLINT" => "USMALLINT",
        "UINTEGER" => "UINTEGER",
        "UBIGINT" => "UBIGINT",
        "UHUGEINT" => "UHUGEINT",
        "REAL" | "FLOAT" => "FLOAT",
        "DOUBLE" | "DOUBLE PRECISION" => "DOUBLE",
        "TEXT" | "STRING" | "VARCHAR" => "VARCHAR",
        "BLOB" | "BYTEA" => "BLOB",
        "DATE" => "DATE",
        "TIME" => "TIME",
        "TIMESTAMP" | "DATETIME" => "TIMESTAMP",
        "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => "TIMESTAMPTZ",
        "INTERVAL" => "INTERVAL",
        "UUID" => "UUID",
        "JSON" => "JSON",
        _ => "",
    };
    if !simple.is_empty() {
        return Ok(simple.to_owned());
    }
    let decimal = normalized
        .strip_prefix("DECIMAL")
        .or_else(|| normalized.strip_prefix("NUMERIC"));
    if let Some(parameters) = decimal {
        let parameters = parameters
            .trim()
            .strip_prefix('(')
            .and_then(|text| text.strip_suffix(')'));
        if let Some(parameters) = parameters {
            let mut parts = parameters.split(',').map(str::trim);
            let precision = parts.next().and_then(|part| part.parse::<u8>().ok());
            let scale = parts.next().and_then(|part| part.parse::<u8>().ok());
            if let (Some(precision), Some(scale)) = (precision, scale) {
                if parts.next().is_none() && (1..=38).contains(&precision) && scale <= precision {
                    return Ok(format!("DECIMAL({precision},{scale})"));
                }
            }
        }
    }
    Err(common::bad(
        "不支持的数据类型；DECIMAL 必须使用 DECIMAL(p,s)，1<=p<=38 且 0<=s<=p",
    ))
}

fn primary_key_type(data_type: &str) -> bool {
    !matches!(data_type, "JSON" | "BLOB")
}

#[cfg(test)]
mod standard_pin_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pinned_governance_is_preserved_without_changing_physical_ddl() {
        let mut model = json!({"layer_id":"layer","table_name":"main.orders","columns":[{"name":"id","data_type":"BIGINT"}],"primary_key":["id"]});
        let ddl = generate_ddl(&model).unwrap();
        model["columns"][0]["standard_id"] = json!("field");
        assert!(generate_ddl(&model).is_err());
        model["columns"][0]["standard_version"] = json!(2);
        model["standard_coverage"] = json!("bound_fields");
        assert_eq!(generate_ddl(&model).unwrap(), ddl);
        model["columns"][0]["standard_version"] = json!(1.5);
        assert!(generate_ddl(&model).is_err());
    }
}
