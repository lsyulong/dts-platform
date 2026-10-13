// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use sqlparser::{
    ast::{
        DataType, Expr, TimezoneInfo, TypedString, Value as SqlValue, Visit, VisitMut, Visitor,
        VisitorMut,
    },
    dialect::DuckDbDialect,
    parser::Parser,
};

use crate::{models::error::AppError, services::query_svc::validate_read_only_sql};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParameterDefinition {
    pub name: String,
    pub data_type: String,
    pub required: bool,
    #[serde(default, deserialize_with = "explicit_default")]
    pub default_value: Option<Value>,
}

fn explicit_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}

pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn sensitive_name(value: &str) -> bool {
    let name = value.to_ascii_lowercase().replace('_', "");
    [
        "password",
        "passwd",
        "secret",
        "token",
        "authorization",
        "credential",
        "apikey",
        "privatekey",
        "connectionstring",
        "dsn",
    ]
    .iter()
    .any(|part| name.contains(part))
        || name == "pwd"
}

fn sensitive_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.trim_start().starts_with("bearer ")
        || (lower.contains("-----begin") && lower.contains("key-----"))
        || ["password=", "passwd=", "token=", "secret=", "api_key="]
            .iter()
            .any(|key| lower.contains(key))
        || value.split_once("://").is_some_and(|(_, rest)| {
            rest.split(['/', '?', '#'])
                .next()
                .is_some_and(|authority| authority.contains('@'))
        })
        || (value.starts_with("eyJ") && value.split('.').count() == 3)
}

pub(crate) fn validate_definitions(definitions: &[ParameterDefinition]) -> Result<(), AppError> {
    if definitions.len() > 32 {
        return Err(bad("最多允许 32 个类型化参数"));
    }
    let mut names = BTreeSet::new();
    for parameter in definitions {
        if !identifier(&parameter.name)
            || sensitive_name(&parameter.name)
            || !names.insert(&parameter.name)
        {
            return Err(bad("参数名无效、重复或包含凭据名称"));
        }
        if ![
            "VARCHAR",
            "BIGINT",
            "DOUBLE",
            "BOOLEAN",
            "DATE",
            "TIMESTAMP",
        ]
        .contains(&parameter.data_type.as_str())
        {
            return Err(bad(
                "参数类型仅支持 VARCHAR/BIGINT/DOUBLE/BOOLEAN/DATE/TIMESTAMP",
            ));
        }
        if let Some(value) = &parameter.default_value {
            literal(parameter, value)?;
        }
    }
    Ok(())
}

pub(crate) fn resolve(
    definitions: &[ParameterDefinition],
    input: &Value,
) -> Result<(Value, BTreeMap<String, Expr>), AppError> {
    validate_definitions(definitions)?;
    let values = input
        .as_object()
        .ok_or_else(|| bad("运行参数必须为 JSON 对象"))?;
    if serde_json::to_vec(input)
        .map_err(|_| bad("参数无法编码"))?
        .len()
        > 16 * 1024
    {
        return Err(bad("运行参数超过 16 KiB"));
    }
    if values.keys().any(|name| {
        !definitions
            .iter()
            .any(|definition| &definition.name == name)
    }) {
        return Err(bad("运行参数包含未声明字段"));
    }
    let mut resolved = Map::new();
    let mut expressions = BTreeMap::new();
    for parameter in definitions {
        let value = values
            .get(&parameter.name)
            .or(parameter.default_value.as_ref())
            .cloned()
            .unwrap_or(Value::Null);
        expressions.insert(parameter.name.clone(), literal(parameter, &value)?);
        resolved.insert(parameter.name.clone(), value);
    }
    Ok((Value::Object(resolved), expressions))
}

fn literal(parameter: &ParameterDefinition, value: &Value) -> Result<Expr, AppError> {
    if value.is_null() {
        return if parameter.required {
            Err(bad("必需参数缺失或为 NULL"))
        } else {
            Ok(Expr::Value(SqlValue::Null.into()))
        };
    }
    let scalar = match parameter.data_type.as_str() {
        "BOOLEAN" => SqlValue::Boolean(
            value
                .as_bool()
                .ok_or_else(|| bad("BOOLEAN 参数必须为布尔值"))?,
        ),
        "BIGINT" => SqlValue::Number(int64(value)?.to_string(), false),
        "DOUBLE" if value.as_f64().is_some_and(f64::is_finite) => {
            SqlValue::Number(value.to_string(), false)
        },
        "VARCHAR" => SqlValue::SingleQuotedString(text(value)?.into()),
        "DATE" => {
            let date = text(value)?;
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| bad("DATE 参数必须为有效的 YYYY-MM-DD"))?;
            return Ok(typed(DataType::Date, date.into()));
        },
        "TIMESTAMP" => return timestamp(text(value)?),
        _ => return Err(bad("参数值与声明类型不一致")),
    };
    Ok(Expr::Value(scalar.into()))
}

fn int64(value: &Value) -> Result<i64, AppError> {
    if let Some(integer) = value.as_i64() {
        return Ok(integer);
    }
    if let Some(text) = value.as_str().filter(|text| text.len() <= 20) {
        if let Ok(integer) = text.parse::<i64>() {
            if integer.to_string() == text {
                return Ok(integer);
            }
        }
    }
    Err(bad(
        "BIGINT 参数必须为 64 位整数，或无前导零的十进制整数字符串",
    ))
}

#[cfg(test)]
mod int64_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_int64_strings_bind_without_javascript_rounding_or_sql_injection() {
        let parameter = ParameterDefinition {
            name: "id".into(),
            data_type: "BIGINT".into(),
            required: true,
            default_value: None,
        };
        for text in [
            "9223372036854775807",
            "-9223372036854775808",
            "9007199254740993",
            "0",
        ] {
            let (_, expressions) =
                resolve(&[parameter.clone()], &json!({"id": text})).expect("exact integer");
            let sql = bind("SELECT $id AS id", &expressions).expect("safe literal");
            assert_eq!(sql, format!("SELECT {text} AS id"));
        }
        for value in [
            json!("9223372036854775808"),
            json!("-9223372036854775809"),
            json!("01"),
            json!("-0"),
            json!("+1"),
            json!("1; DROP TABLE x"),
            json!("1.0"),
            json!(1.5),
        ] {
            assert!(resolve(&[parameter.clone()], &json!({"id": value})).is_err());
        }
    }
}

fn text(value: &Value) -> Result<&str, AppError> {
    let text = value
        .as_str()
        .ok_or_else(|| bad("文本和日期参数必须为字符串"))?;
    if text.len() > 8192 || text.chars().any(char::is_control) || sensitive_value(text) {
        return Err(bad("参数文本过长、包含控制字符或明文凭据"));
    }
    Ok(text)
}

fn typed(data_type: DataType, value: String) -> Expr {
    Expr::TypedString(TypedString {
        data_type,
        value: SqlValue::SingleQuotedString(value).into(),
        uses_odbc_syntax: false,
    })
}

fn timestamp(value: &str) -> Result<Expr, AppError> {
    let instant = if let Ok(value) = chrono::DateTime::parse_from_rfc3339(value) {
        value.naive_utc()
    } else {
        chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
            .map_err(|_| bad("TIMESTAMP 参数必须为有效的日期时间"))?
    };
    Ok(typed(
        DataType::Timestamp(None, TimezoneInfo::None),
        instant.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
    ))
}

struct Placeholders<'a> {
    definitions: &'a [ParameterDefinition],
    used: BTreeSet<String>,
}
impl Visitor for Placeholders<'_> {
    type Break = &'static str;
    fn pre_visit_relation(
        &mut self,
        name: &sqlparser::ast::ObjectName,
    ) -> ControlFlow<Self::Break> {
        if name.to_string().contains('$') {
            ControlFlow::Break("参数不得用作表名或标识符")
        } else {
            ControlFlow::Continue(())
        }
    }
    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        if let Expr::Value(value) = expr {
            if let SqlValue::Placeholder(name) = &value.value {
                let Some(name) = name.strip_prefix('$').filter(|name| identifier(name)) else {
                    return ControlFlow::Break("仅支持表达式中的 $name 参数");
                };
                if !self
                    .definitions
                    .iter()
                    .any(|parameter| parameter.name == name)
                {
                    return ControlFlow::Break("SQL 使用了未声明的参数");
                }
                self.used.insert(name.into());
            }
        }
        ControlFlow::Continue(())
    }
}

pub(crate) fn inspect(
    sql: &str,
    definitions: &[ParameterDefinition],
) -> Result<BTreeSet<String>, AppError> {
    validate_read_only_sql(sql)?;
    let statements = Parser::parse_sql(&DuckDbDialect {}, sql).map_err(|_| bad("SQL 解析失败"))?;
    let mut visitor = Placeholders {
        definitions,
        used: BTreeSet::new(),
    };
    if let ControlFlow::Break(reason) = Visit::visit(&statements, &mut visitor) {
        return Err(bad(reason));
    }
    Ok(visitor.used)
}

struct Binder<'a>(&'a BTreeMap<String, Expr>);
impl VisitorMut for Binder<'_> {
    type Break = &'static str;
    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if let Expr::Value(value) = expr {
            if let SqlValue::Placeholder(name) = &value.value {
                let Some(replacement) = name.strip_prefix('$').and_then(|name| self.0.get(name))
                else {
                    return ControlFlow::Break("SQL 参数尚未绑定");
                };
                *expr = replacement.clone();
            }
        }
        ControlFlow::Continue(())
    }
}

pub(crate) fn bind(sql: &str, expressions: &BTreeMap<String, Expr>) -> Result<String, AppError> {
    let mut statements =
        Parser::parse_sql(&DuckDbDialect {}, sql).map_err(|_| bad("SQL 解析失败"))?;
    if let ControlFlow::Break(reason) = VisitMut::visit(&mut statements, &mut Binder(expressions)) {
        return Err(bad(reason));
    }
    if statements.len() != 1 {
        return Err(bad("仅支持单条只读查询"));
    }
    render_statement(statements.remove(0))
}

pub(crate) fn render_statement(statement: sqlparser::ast::Statement) -> Result<String, AppError> {
    let mut statements = vec![statement];
    let expected_strings = strings(&statements);
    let _: ControlFlow<()> = VisitMut::visit(&mut statements, &mut QuotePrinter);
    let bound = statements[0].to_string();
    validate_read_only_sql(&bound)?;
    let reparsed =
        Parser::parse_sql(&DuckDbDialect {}, &bound).map_err(|_| bad("绑定后的 SQL 无法解析"))?;
    if strings(&reparsed) != expected_strings {
        return Err(bad("SQL 字面值无法安全往返编码"));
    }
    Ok(bound)
}

struct QuotePrinter;
impl VisitorMut for QuotePrinter {
    type Break = ();
    fn pre_visit_value(&mut self, value: &mut SqlValue) -> ControlFlow<()> {
        if let SqlValue::SingleQuotedString(text) = value {
            *text = text.replace('\'', "''");
        }
        ControlFlow::Continue(())
    }
}

fn strings(statements: &[sqlparser::ast::Statement]) -> Vec<String> {
    struct Strings(Vec<String>);
    impl Visitor for Strings {
        type Break = ();
        fn pre_visit_value(&mut self, value: &SqlValue) -> ControlFlow<()> {
            if let SqlValue::SingleQuotedString(value) = value {
                self.0.push(value.clone());
            }
            ControlFlow::Continue(())
        }
    }
    let mut values = Strings(vec![]);
    for statement in statements {
        let _: ControlFlow<()> = Visit::visit(statement, &mut values);
    }
    values.0
}

pub(crate) fn bind_optional_text(
    sql: &str,
    values: &[(&str, Option<&str>)],
) -> Result<String, AppError> {
    let definitions: Vec<_> = values
        .iter()
        .map(|(name, _)| ParameterDefinition {
            name: (*name).into(),
            data_type: "VARCHAR".into(),
            required: false,
            default_value: None,
        })
        .collect();
    validate_definitions(&definitions)?;
    inspect(sql, &definitions)?;
    let input = Value::Object(
        values
            .iter()
            .map(|(name, value)| {
                (
                    (*name).into(),
                    value.map_or(Value::Null, |value| Value::String(value.into())),
                )
            })
            .collect(),
    );
    let (_, expressions) = resolve(&definitions, &input)?;
    bind(sql, &expressions)
}
