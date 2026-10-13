use super::{
    bad,
    definition::{numeric, quote, text},
    types::*,
};
use crate::models::error::AppError;
use dts_backend::repo::metadata_catalog::MetadataColumn;
use sqlparser::{ast::*, dialect::DuckDbDialect, parser::Parser, tokenizer::Token};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
};

pub(super) fn parse(expression: &str) -> Result<Expr, AppError> {
    text(expression, 16000)?;
    let dialect = DuckDbDialect {};
    let mut parser = Parser::new(&dialect)
        .with_recursion_limit(32)
        .try_with_sql(expression)
        .map_err(|_| bad("指标表达式无法解析"))?;
    let value = parser
        .parse_expr()
        .map_err(|_| bad("指标表达式必须是一个合法表达式"))?;
    if parser.peek_token().token != Token::EOF {
        return Err(bad("指标表达式不能包含语句或尾随内容"));
    }
    Ok(value)
}
pub(super) fn number(value: &str) -> Result<String, AppError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() > 2
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        || parts.iter().map(|part| part.len()).sum::<usize>() > 38
        || parts.get(1).is_some_and(|fraction| fraction.len() > 18)
    {
        return Err(bad(
            "指标常量仅接受38位内有限十进制，不能隐式转科学计数近似值",
        ));
    }
    Ok(value.into())
}
pub(super) fn column<'a>(
    name: &str,
    columns: &'a [MetadataColumn],
) -> Result<&'a MetadataColumn, AppError> {
    columns
        .iter()
        .find(|column| column.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| bad("指标引用了真实元数据中不存在的字段"))
}
fn row(value: &Expr, columns: Option<&[MetadataColumn]>, depth: usize) -> Result<String, AppError> {
    if depth > 32 {
        return Err(bad("指标行表达式过深"));
    }
    Ok(match value {
        Expr::Identifier(id) => {
            let name = if let Some(columns) = columns {
                let column = column(&id.value, columns)?;
                if !numeric(&column.data_type) {
                    return Err(bad("数值度量只能引用真实数值字段"));
                }
                column.name.as_str()
            } else {
                &id.value
            };
            quote(name)?
        },
        Expr::Value(value) => match &value.value {
            Value::Number(number_value, _) => number(number_value)?,
            _ => return Err(bad("度量仅接受有限数值字面量")),
        },
        Expr::Nested(value) => format!("({})", row(value, columns, depth + 1)?),
        Expr::UnaryOp { op, expr } if matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) => {
            format!("{op}({})", row(expr, columns, depth + 1)?)
        },
        Expr::BinaryOp { left, op, right } => {
            let left = row(left, columns, depth + 1)?;
            let right = row(right, columns, depth + 1)?;
            match op {
                BinaryOperator::Plus | BinaryOperator::Minus | BinaryOperator::Multiply => {
                    format!("({left} {op} {right})")
                },
                BinaryOperator::Divide => format!("({left} / NULLIF({right},0))"),
                _ => return Err(bad("数值度量只支持加减乘除")),
            }
        },
        _ => return Err(bad("度量行表达式不支持子查询、函数、聚合嵌套或外部访问")),
    })
}
pub(super) fn aggregate(
    expression: &str,
    columns: Option<&[MetadataColumn]>,
) -> Result<(String, Option<String>), AppError> {
    let Expr::Function(function) = parse(expression)? else {
        return Err(bad("原子指标必须是SUM/AVG/COUNT/MIN/MAX单个聚合"));
    };
    if function.name.0.len() != 1
        || function.over.is_some()
        || function.filter.is_some()
        || function.uses_odbc_syntax
        || function.null_treatment.is_some()
        || !function.within_group.is_empty()
        || function.parameters != FunctionArguments::None
    {
        return Err(bad("原子聚合不能包含窗口、过滤子句或附加语法"));
    }
    let name = function.name.0[0]
        .as_ident()
        .ok_or_else(|| bad("聚合函数名无效"))?
        .value
        .to_ascii_uppercase();
    if !matches!(name.as_str(), "SUM" | "AVG" | "COUNT" | "MIN" | "MAX") {
        return Err(bad("原子聚合函数不受支持"));
    }
    let FunctionArguments::List(args) = function.args else {
        return Err(bad("聚合必须有一个字段或数值行表达式参数"));
    };
    if args.args.len() != 1 || args.duplicate_treatment.is_some() || !args.clauses.is_empty() {
        return Err(bad("聚合仅接受一个普通参数，DISTINCT口径尚未发布支持"));
    }
    let argument = match &args.args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) if name == "COUNT" => None,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id))) if name == "COUNT" => {
            Some(quote(if let Some(columns) = columns {
                &column(&id.value, columns)?.name
            } else {
                &id.value
            })?)
        },
        FunctionArg::Unnamed(FunctionArgExpr::Expr(value)) => Some(row(value, columns, 0)?),
        _ => return Err(bad("聚合参数不受支持")),
    };
    Ok((name, argument))
}
fn dimension(unit: Option<&UnitDefinition>) -> BTreeMap<String, i32> {
    let mut result = BTreeMap::new();
    if let Some(unit) = unit {
        if unit.dimension != "dimensionless" {
            result.insert(unit.dimension.clone(), 1);
        }
    }
    result
}
pub(super) fn unit_dimension(unit: Option<&UnitDefinition>) -> BTreeMap<String, i32> {
    dimension(unit)
}
fn combine(
    mut left: BTreeMap<String, i32>,
    right: &BTreeMap<String, i32>,
    sign: i32,
) -> Result<BTreeMap<String, i32>, AppError> {
    for (key, value) in right {
        let updated = left
            .get(key)
            .copied()
            .unwrap_or(0)
            .checked_add(value.checked_mul(sign).ok_or_else(|| bad("计量维度溢出"))?)
            .ok_or_else(|| bad("计量维度溢出"))?;
        if !(-16..=16).contains(&updated) {
            return Err(bad("计量维度指数超出范围"));
        }
        if updated == 0 {
            left.remove(key);
        } else {
            left.insert(key.clone(), updated);
        }
    }
    Ok(left)
}
pub(super) fn formula(
    expression: &str,
    dependencies: &[(String, MetricPlan)],
) -> Result<(String, BTreeMap<String, i32>), AppError> {
    struct References(BTreeSet<String>);
    impl Visitor for References {
        type Break = ();
        fn pre_visit_expr(&mut self, value: &Expr) -> ControlFlow<()> {
            if let Expr::Identifier(id) = value {
                self.0.insert(id.value.to_lowercase());
            }
            ControlFlow::Continue(())
        }
    }
    fn visit(
        value: &Expr,
        deps: &[(String, MetricPlan)],
        depth: usize,
    ) -> Result<(String, BTreeMap<String, i32>), AppError> {
        if depth > 32 {
            return Err(bad("复合表达式过深"));
        }
        match value {
            Expr::Identifier(id) => {
                if deps.is_empty() {
                    text(&id.value, 128)?;
                    return Ok((quote(&id.value)?, BTreeMap::new()));
                }
                let plan = &deps
                    .iter()
                    .find(|(code, _)| code.eq_ignore_ascii_case(&id.value))
                    .ok_or_else(|| bad("复合公式必须按依赖指标的实际编码引用"))?
                    .1;
                let factor = plan
                    .unit
                    .as_ref()
                    .map_or(1.0, |unit| unit.conversion_factor);
                let factor = super::precision::factor(factor)?;
                Ok((
                    if factor == "1" {
                        format!("({})", plan.expression)
                    } else {
                        format!("(({}) * {factor})", plan.expression)
                    },
                    plan.dimension.clone(),
                ))
            },
            Expr::Value(value) => match &value.value {
                Value::Number(value, _) => Ok((number(value)?, BTreeMap::new())),
                _ => Err(bad("复合公式只接受有限数值常量")),
            },
            Expr::Nested(value) => {
                let (sql, dimension) = visit(value, deps, depth + 1)?;
                Ok((format!("({sql})"), dimension))
            },
            Expr::UnaryOp { op, expr }
                if matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) =>
            {
                let (sql, dimension) = visit(expr, deps, depth + 1)?;
                Ok((format!("{op}({sql})"), dimension))
            },
            Expr::BinaryOp { left, op, right } => {
                let (left, left_unit) = visit(left, deps, depth + 1)?;
                let (right, right_unit) = visit(right, deps, depth + 1)?;
                match op {
                    BinaryOperator::Plus | BinaryOperator::Minus => {
                        if !deps.is_empty() && left_unit != right_unit {
                            return Err(bad("加减运算要求相同计量维度，不能默默混合口径"));
                        }
                        Ok((format!("({left} {op} {right})"), left_unit))
                    },
                    BinaryOperator::Multiply => Ok((
                        format!("({left} * {right})"),
                        combine(left_unit, &right_unit, 1)?,
                    )),
                    BinaryOperator::Divide => Ok((
                        format!("({left} / NULLIF({right},0))"),
                        combine(left_unit, &right_unit, -1)?,
                    )),
                    _ => Err(bad("复合指标仅支持加减乘除")),
                }
            },
            _ => Err(bad("复合公式不能包含任意函数、字段、子查询或语句")),
        }
    }
    let expression = parse(expression)?;
    if !dependencies.is_empty() {
        let mut used = References(BTreeSet::new());
        let _ = expression.visit(&mut used);
        if used.0
            != dependencies
                .iter()
                .map(|(code, _)| code.to_lowercase())
                .collect()
        {
            return Err(bad("复合表达式须恰好引用每个选定依赖指标的实际编码"));
        }
    }
    visit(&expression, dependencies, 0)
}

pub(super) fn substitute(
    expression: &str,
    values: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    fn visit(
        value: &Expr,
        values: &BTreeMap<String, String>,
        depth: usize,
    ) -> Result<String, AppError> {
        if depth > 64 {
            return Err(bad("冻结复合表达式过深"));
        }
        Ok(match value {
            Expr::Identifier(id) => values
                .get(&id.value)
                .cloned()
                .ok_or_else(|| bad("冻结公式引用了不存在的真实聚合"))?,
            Expr::Nested(value) => format!("({})", visit(value, values, depth + 1)?),
            Expr::Value(value) => match &value.value {
                Value::Number(value, _) => number(value)?,
                _ => return Err(bad("冻结公式常量不受支持")),
            },
            Expr::UnaryOp { op, expr }
                if matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) =>
            {
                format!("{op}({})", visit(expr, values, depth + 1)?)
            },
            Expr::BinaryOp { left, op, right }
                if matches!(
                    op,
                    BinaryOperator::Plus
                        | BinaryOperator::Minus
                        | BinaryOperator::Multiply
                        | BinaryOperator::Divide
                ) =>
            {
                format!(
                    "({} {op} {})",
                    visit(left, values, depth + 1)?,
                    visit(right, values, depth + 1)?
                )
            },
            Expr::Function(function)
                if function.name.to_string().eq_ignore_ascii_case("NULLIF") =>
            {
                if function.over.is_some()
                    || function.filter.is_some()
                    || function.null_treatment.is_some()
                    || function.uses_odbc_syntax
                    || !function.within_group.is_empty()
                    || !matches!(function.parameters, FunctionArguments::None)
                {
                    return Err(bad("冻结除零保护不能包含附加语法"));
                }
                let FunctionArguments::List(args) = &function.args else {
                    return Err(bad("冻结除零保护参数无效"));
                };
                if args.args.len() != 2
                    || args.duplicate_treatment.is_some()
                    || !args.clauses.is_empty()
                {
                    return Err(bad("冻结除零保护参数无效"));
                }
                let args = args
                    .args
                    .iter()
                    .map(|arg| match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(value)) => {
                            visit(value, values, depth + 1)
                        },
                        _ => Err(bad("冻结公式参数无效")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                format!("NULLIF({},{})", args[0], args[1])
            },
            _ => return Err(bad("冻结公式包含非受控结构")),
        })
    }
    visit(&parse(expression)?, values, 0)
}
