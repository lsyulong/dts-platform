use super::{common, model};
use crate::models::error::AppError;
use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Query,
    UnaryOperator, Value, Visit, Visitor,
};
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;
use std::ops::ControlFlow;

pub fn validate(expression: &str) -> Result<(), AppError> {
    let dialect = DuckDbDialect {};
    let mut parser = Parser::new(&dialect)
        .with_recursion_limit(64)
        .try_with_sql(expression)
        .map_err(|_| common::bad("指标表达式无法解析"))?;
    let expression = parser
        .parse_expr()
        .map_err(|_| common::bad("指标表达式语法无效"))?;
    if parser.peek_token().token != Token::EOF {
        return Err(common::bad(
            "必须输入单个表达式，不能包含语句、别名或尾随内容",
        ));
    }
    match expression.visit(&mut ExpressionGuard { nodes: 0 }) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(message) => Err(common::bad(message)),
    }
}

struct ExpressionGuard {
    nodes: usize,
}

impl Visitor for ExpressionGuard {
    type Break = &'static str;

    fn pre_visit_query(&mut self, _query: &Query) -> ControlFlow<Self::Break> {
        ControlFlow::Break("指标表达式不能包含子查询")
    }

    fn pre_visit_expr(&mut self, expression: &Expr) -> ControlFlow<Self::Break> {
        self.nodes += 1;
        if self.nodes > 2000 {
            return ControlFlow::Break("指标表达式过于复杂");
        }
        let valid = match expression {
            Expr::Identifier(identifier) => {
                common::identifier(&identifier.value, "字段引用").is_ok()
            },
            Expr::CompoundIdentifier(parts) => {
                (2..=3).contains(&parts.len())
                    && parts
                        .iter()
                        .all(|part| common::identifier(&part.value, "字段引用").is_ok())
            },
            Expr::BinaryOp { op, .. } => matches!(
                op,
                BinaryOperator::Plus
                    | BinaryOperator::Minus
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Modulo
                    | BinaryOperator::DuckIntegerDivide
                    | BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Gt
                    | BinaryOperator::Lt
                    | BinaryOperator::GtEq
                    | BinaryOperator::LtEq
                    | BinaryOperator::And
                    | BinaryOperator::Or
            ),
            Expr::UnaryOp { op, .. } => matches!(
                op,
                UnaryOperator::Plus | UnaryOperator::Minus | UnaryOperator::Not
            ),
            Expr::Value(value) => match &value.value {
                Value::Number(number, _) => number.parse::<f64>().is_ok_and(f64::is_finite),
                Value::SingleQuotedString(_) | Value::Boolean(_) | Value::Null => true,
                _ => false,
            },
            Expr::Function(function) => valid_function(function),
            Expr::Cast {
                data_type,
                array,
                format,
                ..
            } => {
                !array
                    && format.is_none()
                    && model::canonical_data_type(&data_type.to_string()).is_ok()
            },
            Expr::TypedString(value) => matches!(
                value.data_type.to_string().as_str(),
                "DATE" | "TIME" | "TIMESTAMP"
            ),
            Expr::Nested(_)
            | Expr::Case { .. }
            | Expr::IsNull(_)
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
            | Expr::ILike { .. }
            | Expr::Extract { .. }
            | Expr::Ceil { .. }
            | Expr::Floor { .. }
            | Expr::Substring { .. }
            | Expr::Trim { .. } => true,
            _ => false,
        };
        if valid {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break("表达式包含不支持的函数、运算符或结构；不允许外部访问、窗口及子查询")
        }
    }
}

fn valid_function(function: &Function) -> bool {
    if function.name.0.len() != 1
        || function.uses_odbc_syntax
        || function.over.is_some()
        || function.null_treatment.is_some()
        || !function.within_group.is_empty()
        || !matches!(&function.parameters, FunctionArguments::None)
    {
        return false;
    }
    let Some(name) = function.name.0.first().and_then(|part| part.as_ident()) else {
        return false;
    };
    let name = name.value.to_ascii_lowercase();
    let aggregate = matches!(name.as_str(), "sum" | "count" | "avg" | "min" | "max");
    let (min, max) = match name.as_str() {
        "sum" | "count" | "avg" | "min" | "max" | "abs" | "sqrt" | "lower" | "upper" => (1, 1),
        "round" => (1, 2),
        "nullif" | "power" => (2, 2),
        "coalesce" | "greatest" | "least" => (2, 64),
        _ => return false,
    };
    let FunctionArguments::List(args) = &function.args else {
        return false;
    };
    if !(min..=max).contains(&args.args.len())
        || !args.clauses.is_empty()
        || (!aggregate && (function.filter.is_some() || args.duplicate_treatment.is_some()))
    {
        return false;
    }
    args.args.iter().all(|arg| match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(_)) => true,
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {
            name == "count" && args.args.len() == 1 && args.duplicate_treatment.is_none()
        },
        _ => false,
    })
}
