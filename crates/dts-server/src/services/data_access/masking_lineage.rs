use super::{ast, denied, expression::ColumnGuard};
use crate::models::error::AppError;
use dts_backend::repo::data_access::DataAccessTarget;
use sqlparser::ast::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn masking_table(sql: &str) -> Result<Vec<String>, AppError> {
    Ok(ast::parse(sql)?.table)
}

/// Resolve only direct projections. A masked source used in computation or in
/// row selection/order is refused so output replacement cannot be bypassed.
pub(crate) fn masking_outputs(
    sql: &str,
    target: &DataAccessTarget,
    sensitive: &BTreeSet<String>,
) -> Result<Vec<(String, String)>, AppError> {
    let parsed = ast::parse(sql)?;
    if !ast::matches(&parsed, target) {
        return Err(denied("脱敏绑定不属于实际查询表"));
    }
    ast::columns(&parsed, target)?;
    let SetExpr::Select(select) = &*parsed.query.body else {
        unreachable!()
    };
    let used = |expr: &Expr| -> Result<BTreeSet<String>, AppError> {
        let mut guard = ColumnGuard::new(target, &parsed.table, parsed.alias.as_deref());
        guard.check(expr)?;
        Ok(guard.used)
    };
    let forbidden = |expr: &Expr| -> Result<(), AppError> {
        if used(expr)?.iter().any(|column| sensitive.contains(column)) {
            Err(denied("脱敏字段不能用于筛选、分组、排序或派生表达式"))
        } else {
            Ok(())
        }
    };
    let mut outputs = Vec::new();
    let mut aliases = BTreeMap::new();
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                let references = used(expr)?;
                let masked: Vec<_> = references
                    .iter()
                    .filter(|name| sensitive.contains(*name))
                    .collect();
                let direct = match expr {
                    Expr::Identifier(id) => Some(id.value.as_str()),
                    Expr::CompoundIdentifier(parts) => parts.last().map(|id| id.value.as_str()),
                    _ => None,
                };
                if !masked.is_empty() {
                    let source = direct
                        .and_then(|name| {
                            target
                                .columns
                                .iter()
                                .find(|column| column.name.eq_ignore_ascii_case(name))
                        })
                        .filter(|column| masked.len() == 1 && masked[0].as_str() == column.name)
                        .ok_or_else(|| denied("脱敏字段的派生表达式暂不允许输出"))?;
                    let output = match item {
                        SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                        _ => source.name.clone(),
                    };
                    outputs.push((output, source.name.clone()));
                }
                if let SelectItem::ExprWithAlias { alias, .. } = item {
                    aliases.insert(alias.value.to_ascii_lowercase(), expr);
                }
            },
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                outputs.extend(
                    target
                        .columns
                        .iter()
                        .filter(|column| sensitive.contains(&column.name))
                        .map(|column| (column.name.clone(), column.name.clone())),
                );
            },
        }
    }
    if select.distinct.is_some() && !outputs.is_empty() {
        return Err(denied("脱敏字段的DISTINCT查询暂不允许"));
    }
    if let Some(expr) = &select.selection {
        forbidden(expr)?;
    }
    if let GroupByExpr::Expressions(expressions, _) = &select.group_by {
        for expr in expressions {
            reject_position(expr)?;
            forbidden(expr)?;
        }
    }
    if let Some(order) = &parsed.query.order_by {
        let OrderByKind::Expressions(expressions) = &order.kind else {
            return Err(denied("脱敏不支持ORDER BY ALL"));
        };
        for expr in expressions {
            reject_position(&expr.expr)?;
            let value = match &expr.expr {
                Expr::Identifier(id) => aliases
                    .get(&id.value.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(&expr.expr),
                _ => &expr.expr,
            };
            forbidden(value)?;
        }
    }
    let mut names = BTreeSet::new();
    if outputs
        .iter()
        .any(|(name, _)| !names.insert(name.to_ascii_lowercase()))
    {
        return Err(denied("脱敏输出字段重复，请使用唯一别名"));
    }
    Ok(outputs)
}

fn reject_position(expr: &Expr) -> Result<(), AppError> {
    if matches!(expr,Expr::Value(value) if matches!(&value.value,Value::Number(_, _))) {
        Err(denied("脱敏查询的排序和分组须使用明确字段，不支持数字位置"))
    } else {
        Ok(())
    }
}
