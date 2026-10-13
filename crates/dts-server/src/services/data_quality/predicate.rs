use super::{FrozenColumn, bad, snapshot::identifier};
use crate::{models::error::AppError, services::product_domain::warehouse::validate_expression};
use serde::Deserialize;
use serde_json::Value;
use sqlparser::{
    ast::{BinaryOperator, Expr, Query, UnaryOperator, Value as SqlValue, Visit, Visitor},
    dialect::DuckDbDialect,
    parser::Parser,
};
use std::{collections::HashSet, ops::ControlFlow};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Condition {
    column: String,
    operator: String,
    #[serde(default)]
    value: Value,
}

pub(super) fn compile(
    rule_type: &str,
    value: &Value,
    columns: &[FrozenColumn],
    target: &str,
) -> Result<String, AppError> {
    if rule_type == "expression" && value.is_object() {
        if value.as_object().is_none_or(|values| values.len() != 1) {
            return Err(bad("表达式条件只支持expression字段"));
        }
        let expression = value["expression"]
            .as_str()
            .filter(|text| !text.is_empty() && text.len() <= 16_000)
            .ok_or_else(|| bad("expression必须是有长度限制的单个布尔表达式"))?;
        bind_expression(expression, columns, target)?;
        return Ok(expression.into());
    }
    let conditions: Vec<Condition> = serde_json::from_value(value.clone())
        .map_err(|_| bad("conditions必须是column/operator/value条件数组"))?;
    if conditions.is_empty() || conditions.len() > 64 {
        return Err(bad("range/expression规则必须包含1至64个真实通过条件"));
    }
    let mut clauses = Vec::with_capacity(conditions.len());
    let mut lower: Option<(&Value, bool)> = None;
    let mut upper: Option<(&Value, bool)> = None;
    for condition in &conditions {
        identifier(&condition.column)?;
        let column = columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(&condition.column))
            .ok_or_else(|| bad("条件引用的字段不在真实元数据范围"))?;
        if rule_type == "range" {
            if !column.name.eq_ignore_ascii_case(target)
                || !numeric(&column.data_type)
                || !condition.value.is_number()
            {
                return Err(bad("range仅支持目标数值字段的数值上下界"));
            }
            let bound = match condition.operator.as_str() {
                "gt" | "gte" => &mut lower,
                "lt" | "lte" => &mut upper,
                _ => return Err(bad("range边界必须使用gt/gte/lt/lte")),
            };
            if bound.is_some() {
                return Err(bad("range只能设置一个上界和一个下界"));
            }
            *bound = Some((
                &condition.value,
                matches!(condition.operator.as_str(), "gt" | "lt"),
            ));
        }
        let quoted = format!("\"{}\"", column.name);
        clauses.push(match condition.operator.as_str() {
            "is_null" | "not_null" => {
                if !condition.value.is_null() && condition.value != Value::String(String::new()) {
                    return Err(bad("空值条件不能附带一个被忽略的值"));
                }
                format!(
                    "{quoted} IS {}NULL",
                    if condition.operator == "not_null" {
                        "NOT "
                    } else {
                        ""
                    }
                )
            },
            "in" => {
                let values = condition
                    .value
                    .as_array()
                    .filter(|values| !values.is_empty() && values.len() <= 100)
                    .ok_or_else(|| bad("in条件必须包含1至100个标量"))?;
                let mut seen = HashSet::new();
                let values = values
                    .iter()
                    .map(|value| {
                        let literal = literal(value, &column.data_type)?;
                        if !seen.insert(literal.clone()) {
                            return Err(bad("in条件不能重复"));
                        }
                        Ok(literal)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                format!("{quoted} IN ({})", values.join(","))
            },
            operator => {
                let operator = match operator {
                    "eq" => "=",
                    "ne" => "<>",
                    "gt" => ">",
                    "gte" => ">=",
                    "lt" => "<",
                    "lte" => "<=",
                    _ => return Err(bad("质量条件运算符不受支持")),
                };
                format!(
                    "{quoted} {operator} {}",
                    literal(&condition.value, &column.data_type)?
                )
            },
        });
    }
    if let (Some((low, low_strict)), Some((high, high_strict))) = (lower, upper) {
        let integer = |value: &Value| {
            value
                .as_i64()
                .map(i128::from)
                .or_else(|| value.as_u64().map(i128::from))
        };
        let ordering = match (integer(low), integer(high)) {
            (Some(low), Some(high)) => Some(low.cmp(&high)),
            _ => low
                .as_f64()
                .zip(high.as_f64())
                .and_then(|(low, high)| low.partial_cmp(&high)),
        }
        .ok_or_else(|| bad("range上下界必须是可比较的有限数字"))?;
        if ordering.is_gt() || (ordering.is_eq() && (low_strict || high_strict)) {
            return Err(bad("range上下界为空或顺序颠倒"));
        }
    }
    let predicate = clauses.join(" AND ");
    bind_expression(&predicate, columns, target)?;
    Ok(predicate)
}

fn literal(value: &Value, data_type: &str) -> Result<String, AppError> {
    match value {
        Value::Number(number)
            if numeric(data_type) && number.as_f64().is_some_and(f64::is_finite) =>
        {
            Ok(number.to_string())
        },
        Value::Bool(value) if data_type == "BOOLEAN" => {
            Ok(if *value { "TRUE" } else { "FALSE" }.into())
        },
        Value::String(value)
            if [
                "VARCHAR",
                "DATE",
                "TIME",
                "TIMESTAMP",
                "TIMESTAMPTZ",
                "UUID",
            ]
            .contains(&data_type) =>
        {
            if value.len() > 4096 || value.chars().any(char::is_control) {
                return Err(bad("质量条件文本过长或包含控制字符"));
            }
            let quoted = format!("'{}'", value.replace('\'', "''"));
            Ok(if data_type == "VARCHAR" {
                quoted
            } else {
                format!("CAST({quoted} AS {data_type})")
            })
        },
        _ => Err(bad(
            "条件值必须是与真实字段类型匹配的有限数字、布尔值或文本；NULL请使用is_null",
        )),
    }
}

pub(super) fn numeric(data_type: &str) -> bool {
    matches!(
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
    ) || data_type.starts_with("DECIMAL(")
}

fn bind_expression(
    expression: &str,
    columns: &[FrozenColumn],
    target: &str,
) -> Result<(), AppError> {
    validate_expression(expression)?;
    let dialect = DuckDbDialect {};
    let expr = Parser::new(&dialect)
        .with_recursion_limit(64)
        .try_with_sql(expression)
        .and_then(|mut parser| parser.parse_expr())
        .map_err(|_| bad("质量谓词语法无效"))?;
    if !boolean_predicate(&expr, columns) {
        return Err(bad(
            "质量通过条件必须返回布尔判断，不能直接使用数值或文本表达式",
        ));
    }
    struct Binding<'a> {
        columns: &'a [FrozenColumn],
        used: HashSet<String>,
    }
    impl Visitor for Binding<'_> {
        type Break = &'static str;
        fn pre_visit_query(&mut self, _: &Query) -> ControlFlow<Self::Break> {
            ControlFlow::Break("质量谓词不允许子查询")
        }
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
            match expr {
                Expr::Identifier(id) => {
                    if !self
                        .columns
                        .iter()
                        .any(|column| column.name.eq_ignore_ascii_case(&id.value))
                    {
                        return ControlFlow::Break("质量谓词必须引用冻结元数据中的真实列");
                    }
                    self.used.insert(id.value.to_ascii_lowercase());
                    ControlFlow::Continue(())
                },
                Expr::CompoundIdentifier(_) => {
                    ControlFlow::Break("质量谓词只允许未限定的本表字段引用")
                },
                Expr::Function(function)
                    if function
                        .name
                        .0
                        .first()
                        .and_then(|name| name.as_ident())
                        .is_some_and(|name| {
                            matches!(
                                name.value.to_ascii_lowercase().as_str(),
                                "sum" | "count" | "avg" | "min" | "max"
                            )
                        }) =>
                {
                    ControlFlow::Break("质量通过条件必须逐行判断，不允许聚合函数")
                },
                _ => ControlFlow::Continue(()),
            }
        }
    }
    let mut binding = Binding {
        columns,
        used: HashSet::new(),
    };
    if let ControlFlow::Break(error) = expr.visit(&mut binding) {
        return Err(bad(error));
    }
    if !binding.used.contains(&target.to_ascii_lowercase()) {
        return Err(bad(
            "质量通过条件必须实际引用规则目标字段，不能只使用常量或其他字段",
        ));
    }
    Ok(())
}

fn boolean_predicate(expr: &Expr, columns: &[FrozenColumn]) -> bool {
    match expr {
        Expr::Nested(expr) => boolean_predicate(expr, columns),
        Expr::Identifier(id) => columns.iter().any(|column| {
            column.name.eq_ignore_ascii_case(&id.value) && column.data_type == "BOOLEAN"
        }),
        Expr::Value(value) => matches!(&value.value, SqlValue::Boolean(_)),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => boolean_predicate(expr, columns),
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Gt
            | BinaryOperator::Lt
            | BinaryOperator::GtEq
            | BinaryOperator::LtEq => true,
            BinaryOperator::And | BinaryOperator::Or => {
                boolean_predicate(left, columns) && boolean_predicate(right, columns)
            },
            _ => false,
        },
        Expr::IsNull(_)
        | Expr::IsNotNull(_)
        | Expr::IsTrue(_)
        | Expr::IsNotTrue(_)
        | Expr::IsFalse(_)
        | Expr::IsNotFalse(_)
        | Expr::IsDistinctFrom(_, _)
        | Expr::IsNotDistinctFrom(_, _)
        | Expr::InList { .. }
        | Expr::Between { .. }
        | Expr::Like { .. }
        | Expr::ILike { .. } => true,
        Expr::Cast { data_type, .. } => data_type.to_string().eq_ignore_ascii_case("BOOLEAN"),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.is_none()
                && conditions.iter().all(|branch| {
                    boolean_predicate(&branch.condition, columns)
                        && boolean_predicate(&branch.result, columns)
                })
                && else_result
                    .as_ref()
                    .is_some_and(|expr| boolean_predicate(expr, columns))
        },
        _ => false,
    }
}
