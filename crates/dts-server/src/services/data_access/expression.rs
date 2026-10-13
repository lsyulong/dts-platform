use super::denied;
use crate::{models::error::AppError, services::product_domain::warehouse::canonical_data_type};
use dts_backend::repo::data_access::DataAccessTarget;
use sqlparser::ast::*;
use std::{collections::BTreeSet, ops::ControlFlow};

pub(super) struct ColumnGuard<'a> {
    target: &'a DataAccessTarget,
    table: &'a [String],
    alias: Option<&'a str>,
    pub used: BTreeSet<String>,
    nodes: usize,
}

impl<'a> ColumnGuard<'a> {
    pub fn new(target: &'a DataAccessTarget, table: &'a [String], alias: Option<&'a str>) -> Self {
        Self {
            target,
            table,
            alias,
            used: BTreeSet::new(),
            nodes: 0,
        }
    }
    pub fn all_columns(&mut self) {
        self.used
            .extend(self.target.columns.iter().map(|column| column.name.clone()));
    }
    pub fn qualifier(&self, parts: &[String]) -> Result<(), AppError> {
        let same = |expected: &[String]| {
            parts.len() == expected.len()
                && parts
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
        };
        if same(self.table)
            || same(&self.table[1..])
            || same(&self.table[2..])
            || (parts.len() == 1
                && self
                    .alias
                    .is_some_and(|alias| alias.eq_ignore_ascii_case(&parts[0])))
        {
            Ok(())
        } else {
            Err(denied("字段限定名不属于当前真实数据表"))
        }
    }
    fn column(&mut self, name: &str) -> ControlFlow<&'static str> {
        if let Some(column) = self
            .target
            .columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(name))
        {
            self.used.insert(column.name.clone());
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break("查询引用了未采集或不存在的字段")
        }
    }
    pub fn check(&mut self, expr: &Expr) -> Result<(), AppError> {
        match expr.visit(self) {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(message) => Err(denied(message)),
        }
    }
    fn function(&mut self, function: &Function) -> bool {
        if function.name.0.len() != 1
            || function.over.is_some()
            || function.filter.is_some()
            || function.null_treatment.is_some()
            || function.uses_odbc_syntax
            || !function.within_group.is_empty()
            || !matches!(&function.parameters, FunctionArguments::None)
        {
            return false;
        }
        let Some(name) = function.name.0.first().and_then(|part| part.as_ident()) else {
            return false;
        };
        let name = name.value.to_ascii_lowercase();
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
        if !(min..=max).contains(&args.args.len()) || !args.clauses.is_empty() {
            return false;
        }
        for arg in &args.args {
            match arg {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(_)) => {},
                FunctionArg::Unnamed(FunctionArgExpr::Wildcard)
                    if name == "count"
                        && args.args.len() == 1
                        && args.duplicate_treatment.is_none() =>
                {
                    self.all_columns()
                },
                _ => return false,
            }
        }
        true
    }
}

impl Visitor for ColumnGuard<'_> {
    type Break = &'static str;
    fn pre_visit_query(&mut self, _: &Query) -> ControlFlow<Self::Break> {
        ControlFlow::Break("字段授权查询不允许嵌套子查询")
    }
    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        self.nodes += 1;
        if self.nodes > 2000 {
            return ControlFlow::Break("查询表达式超过复杂度上限");
        }
        let valid = match expr {
            Expr::Identifier(id) => return self.column(&id.value),
            Expr::CompoundIdentifier(parts) => {
                let Some((column, prefix)) = parts.split_last() else {
                    return ControlFlow::Break("字段引用为空");
                };
                let prefix = prefix
                    .iter()
                    .map(|part| part.value.clone())
                    .collect::<Vec<_>>();
                if self.qualifier(&prefix).is_err() {
                    return ControlFlow::Break("字段限定名不属于已授权的真实表");
                }
                return self.column(&column.value);
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
                Value::Number(value, _) => value.parse::<f64>().is_ok_and(f64::is_finite),
                Value::SingleQuotedString(value) => {
                    value.len() <= 4096 && !value.chars().any(char::is_control)
                },
                Value::Boolean(_) | Value::Null => true,
                _ => false,
            },
            Expr::Function(function) => self.function(function),
            Expr::Cast {
                data_type,
                array,
                format,
                ..
            } => !array && format.is_none() && canonical_data_type(&data_type.to_string()).is_ok(),
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
            ControlFlow::Break("查询包含不支持的函数、窗口、占位符或表达式")
        }
    }
}
