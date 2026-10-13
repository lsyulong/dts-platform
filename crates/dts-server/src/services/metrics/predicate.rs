use super::{
    bad,
    definition::{numeric, quote},
    expression::{column, parse},
};
use crate::models::error::AppError;
use dts_backend::repo::metadata_catalog::MetadataColumn;
use sqlparser::ast::{BinaryOperator, Expr, UnaryOperator, Value};

fn literal(value: &Expr, data_type: Option<&str>) -> Result<String, AppError> {
    let value = match value {
        Expr::Nested(value) => return literal(value, data_type),
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr,
        } => {
            let value = literal(expr, data_type)?;
            if !value.parse::<f64>().is_ok_and(f64::is_finite) {
                return Err(bad("修饰条件负号仅适用于有限数值"));
            }
            return Ok(format!("-({value})"));
        },
        Expr::Value(value) => &value.value,
        _ => return Err(bad("修饰条件必须比较实际字段与常量")),
    };
    Ok(match value {
        Value::Number(value, _) => {
            if data_type.is_some_and(|ty| !numeric(ty)) {
                return Err(bad("修饰数值常量与实际字段类型不符"));
            }
            super::expression::number(value)?
        },
        Value::Boolean(value) => {
            if data_type.is_some_and(|ty| !ty.eq_ignore_ascii_case("BOOLEAN")) {
                return Err(bad("修饰布尔常量与实际字段类型不符"));
            }
            value.to_string()
        },
        Value::SingleQuotedString(value) => {
            if value.len() > 2048 || value.chars().any(char::is_control) {
                return Err(bad("修饰文本过长或包含控制字符"));
            }
            if let Some(ty) = data_type {
                if ty.eq_ignore_ascii_case("DATE") {
                    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| bad("日期条件须使用YYYY-MM-DD"))?;
                } else if !matches!(
                    ty.to_ascii_uppercase().as_str(),
                    "VARCHAR" | "TEXT" | "STRING"
                ) {
                    return Err(bad("修饰文本常量与实际字段类型不符"));
                }
            }
            format!("'{}'", value.replace('\'', "''"))
        },
        _ => return Err(bad("修饰条件常量不受支持，请用IS NULL判断空值")),
    })
}
fn field(
    value: &Expr,
    bound: Option<(&str, &[MetadataColumn])>,
) -> Result<(String, Option<String>), AppError> {
    let Expr::Identifier(id) = value else {
        return Err(bad("修饰条件只接受单个实际维度字段"));
    };
    if let Some((dimension, columns)) = bound {
        let column = column(&id.value, columns)?;
        if !column.name.eq_ignore_ascii_case(dimension) {
            return Err(bad("条件只能引用此修饰词绑定的实际维度"));
        }
        Ok((quote(&column.name)?, Some(column.data_type.clone())))
    } else {
        Ok((quote(&id.value)?, None))
    }
}
fn visit(
    value: &Expr,
    bound: Option<(&str, &[MetadataColumn])>,
    depth: usize,
) -> Result<String, AppError> {
    if depth > 32 {
        return Err(bad("修饰条件过深"));
    }
    Ok(match value {
        Expr::Nested(value) => format!("({})", visit(value, bound, depth + 1)?),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => format!("NOT ({})", visit(expr, bound, depth + 1)?),
        Expr::BinaryOp { left, op, right }
            if matches!(op, BinaryOperator::And | BinaryOperator::Or) =>
        {
            format!(
                "({} {op} {})",
                visit(left, bound, depth + 1)?,
                visit(right, bound, depth + 1)?
            )
        },
        Expr::BinaryOp { left, op, right }
            if matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq
            ) =>
        {
            let (field, ty) = field(left, bound)?;
            format!("({field} {op} {})", literal(right, ty.as_deref())?)
        },
        Expr::IsNull(value) => format!("({} IS NULL)", field(value, bound)?.0),
        Expr::IsNotNull(value) => format!("({} IS NOT NULL)", field(value, bound)?.0),
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            if list.is_empty() || list.len() > 100 {
                return Err(bad("修饰IN条件须有1至100个实际值"));
            }
            let (field, ty) = field(expr, bound)?;
            let values = list
                .iter()
                .map(|value| literal(value, ty.as_deref()))
                .collect::<Result<Vec<_>, _>>()?;
            format!(
                "({field} {}IN ({}))",
                if *negated { "NOT " } else { "" },
                values.join(",")
            )
        },
        _ => {
            return Err(bad(
                "修饰条件仅支持比较、IN、空值和布尔连接，不能执行任意SQL",
            ));
        },
    })
}
pub(super) fn validate_syntax(condition: &str) -> Result<(), AppError> {
    visit(&parse(condition)?, None, 0).map(|_| ())
}
pub(super) fn compile(
    condition: &str,
    dimension: &str,
    columns: &[MetadataColumn],
) -> Result<String, AppError> {
    column(dimension, columns)?;
    visit(&parse(condition)?, Some((dimension, columns)), 0)
}
