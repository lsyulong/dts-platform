use super::{bad, denied, expression::ColumnGuard};
use crate::{models::error::AppError, services::query_svc::validate_read_only_sql};
use dts_backend::repo::data_access::DataAccessTarget;
use sqlparser::{
    ast::*,
    dialect::DuckDbDialect,
    keywords::Keyword,
    parser::Parser,
    tokenizer::{Token, Tokenizer},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ParsedQuery {
    pub query: Box<Query>,
    pub table: Vec<String>,
    pub alias: Option<String>,
}

pub(super) fn parse(sql: &str) -> Result<ParsedQuery, AppError> {
    if sql.is_empty() || sql.len() > 65536 {
        return Err(bad("查询长度须在1至65536字节"));
    }
    validate_read_only_sql(sql)?;
    reject_limit_all(sql)?;
    let mut statements = Parser::new(&DuckDbDialect {})
        .with_recursion_limit(64)
        .try_with_sql(sql)
        .and_then(|mut parser| parser.parse_statements())
        .map_err(|_| bad("查询语法无效"))?;
    if statements.len() != 1 {
        return Err(denied("仅支持单条受控SELECT"));
    }
    let Statement::Query(query) = statements.remove(0) else {
        return Err(denied("仅支持受控SELECT"));
    };
    if query.with.is_some()
        || query.fetch.is_some()
        || query.for_clause.is_some()
        || query.format_clause.is_some()
        || query.settings.is_some()
        || !query.pipe_operators.is_empty()
        || !query.locks.is_empty()
    {
        return Err(denied("字段授权查询暂不支持CTE、子查询及高级查询修饰"));
    }
    let SetExpr::Select(select) = &*query.body else {
        return Err(denied("字段授权查询暂不支持集合操作或子查询"));
    };
    if select.from.len() != 1
        || !select.from[0].joins.is_empty()
        || select.into.is_some()
        || select.top.is_some()
        || select.exclude.is_some()
        || select.prewhere.is_some()
        || select.having.is_some()
        || select.qualify.is_some()
        || !select.connect_by.is_empty()
        || !select.named_window.is_empty()
        || !select.lateral_views.is_empty()
        || !select.cluster_by.is_empty()
        || !select.distribute_by.is_empty()
        || !select.sort_by.is_empty()
        || select.value_table_mode.is_some()
        || select.optimizer_hint.is_some()
        || select.select_modifiers.is_some()
        || !matches!(&select.group_by,GroupByExpr::Expressions(_,modifiers) if modifiers.is_empty())
        || select
            .distinct
            .as_ref()
            .is_some_and(|distinct| !matches!(distinct, Distinct::Distinct))
    {
        return Err(denied(
            "字段授权查询只支持单表投影、WHERE、GROUP BY、ORDER BY和LIMIT",
        ));
    }
    let TableFactor::Table {
        name,
        alias,
        args: None,
        with_hints,
        version,
        partitions,
        json_path,
        with_ordinality,
        sample,
        index_hints,
    } = &select.from[0].relation
    else {
        return Err(denied("禁止外部表函数、派生表及其他表来源"));
    };
    if !with_hints.is_empty()
        || version.is_some()
        || !partitions.is_empty()
        || json_path.is_some()
        || *with_ordinality
        || sample.is_some()
        || !index_hints.is_empty()
        || alias
            .as_ref()
            .is_some_and(|alias| !alias.columns.is_empty())
    {
        return Err(denied("不支持表版本、提示、采样及字段重命名"));
    }
    let table = object_parts(name)?;
    if table.len() != 3 {
        return Err(denied("普通字段授权查询须明确使用catalog.schema.table"));
    }
    limits(&query)?;
    Ok(ParsedQuery {
        table,
        alias: alias.as_ref().map(|alias| alias.name.value.clone()),
        query,
    })
}

pub(super) fn matches(parsed: &ParsedQuery, target: &DataAccessTarget) -> bool {
    parsed
        .table
        .iter()
        .zip([
            &target.binding.table_catalog,
            &target.binding.schema_name,
            &target.binding.table_name,
        ])
        .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
}

pub(super) fn columns(
    parsed: &ParsedQuery,
    target: &DataAccessTarget,
) -> Result<BTreeSet<String>, AppError> {
    let SetExpr::Select(select) = &*parsed.query.body else {
        return Err(denied("只支持单表查询"));
    };
    let mut aliases = BTreeMap::new();
    let mut guard = ColumnGuard::new(target, &parsed.table, parsed.alias.as_deref());
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) => guard.check(expr)?,
            SelectItem::ExprWithAlias { expr, alias } => {
                let name = alias.value.to_ascii_lowercase();
                if target
                    .columns
                    .iter()
                    .any(|column| column.name.eq_ignore_ascii_case(&name))
                {
                    return Err(denied("投影别名不能覆盖实际字段名"));
                }
                guard.check(expr)?;
                if aliases.insert(name, expr).is_some() {
                    return Err(denied("投影别名重复"));
                }
            },
            SelectItem::Wildcard(options) => {
                wildcard(options)?;
                guard.all_columns();
            },
            SelectItem::QualifiedWildcard(
                SelectItemQualifiedWildcardKind::ObjectName(name),
                options,
            ) => {
                wildcard(options)?;
                guard.qualifier(&object_parts(name)?)?;
                guard.all_columns();
            },
            _ => return Err(denied("不支持表达式星号")),
        }
    }
    if let Some(selection) = &select.selection {
        guard.check(selection)?;
    }
    if let GroupByExpr::Expressions(expressions, _) = &select.group_by {
        for expression in expressions {
            guard.check(expression)?;
        }
    }
    if let Some(order) = &parsed.query.order_by {
        if order.interpolate.is_some() {
            return Err(denied("不支持ORDER BY插值"));
        }
        let OrderByKind::Expressions(expressions) = &order.kind else {
            return Err(denied("不支持ORDER BY ALL"));
        };
        for expression in expressions {
            if expression.with_fill.is_some() {
                return Err(denied("不支持ORDER BY填充"));
            }
            if let Expr::Identifier(id) = &expression.expr {
                if let Some(expr) = aliases.get(&id.value.to_ascii_lowercase()) {
                    guard.check(expr)?;
                    continue;
                }
            }
            guard.check(&expression.expr)?;
        }
    }
    // Even SELECT 1 reveals table cardinality through returned rows.
    if guard.used.is_empty() {
        guard.all_columns();
    }
    Ok(guard.used)
}

fn object_parts(name: &ObjectName) -> Result<Vec<String>, AppError> {
    name.0
        .iter()
        .map(|part| {
            part.as_ident()
                .map(|ident| ident.value.clone())
                .filter(|value| {
                    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
                })
                .ok_or_else(|| denied("实际表名称无效"))
        })
        .collect()
}

fn wildcard(options: &WildcardAdditionalOptions) -> Result<(), AppError> {
    if options.opt_ilike.is_some()
        || options.opt_exclude.is_some()
        || options.opt_except.is_some()
        || options.opt_replace.is_some()
        || options.opt_rename.is_some()
    {
        Err(denied("星号字段排除、替换和重命名尚未支持"))
    } else {
        Ok(())
    }
}

fn limits(query: &Query) -> Result<(), AppError> {
    let number = |expr: &Expr, max: u64| match expr {
        Expr::Value(value) => match &value.value {
            Value::Number(value, false)
                if value.bytes().all(|byte| byte.is_ascii_digit())
                    && value.parse::<u64>().is_ok_and(|value| value <= max) =>
            {
                Ok(())
            },
            _ => Err(denied("LIMIT/OFFSET须为有界非负整数")),
        },
        _ => Err(denied("LIMIT/OFFSET不能使用字段或表达式")),
    };
    if let Some(limit) = &query.limit_clause {
        match limit {
            LimitClause::LimitOffset {
                limit,
                offset,
                limit_by,
            } if limit_by.is_empty() => {
                if limit.is_none() && offset.is_none() {
                    return Err(denied("LIMIT ALL尚未支持"));
                }
                if let Some(limit) = limit {
                    number(limit, 10000)?;
                }
                if let Some(offset) = offset {
                    number(&offset.value, 1_000_000)?;
                }
            },
            LimitClause::OffsetCommaLimit { offset, limit } => {
                number(offset, 1_000_000)?;
                number(limit, 10000)?;
            },
            _ => return Err(denied("不支持LIMIT BY")),
        }
    }
    Ok(())
}

fn reject_limit_all(sql: &str) -> Result<(), AppError> {
    // sqlparser 0.61 normalizes LIMIT ALL to no limit_clause. Inspect real tokens
    // before this information is lost; strings and quoted identifiers stay data.
    let tokens = Tokenizer::new(&DuckDbDialect {}, sql)
        .tokenize()
        .map_err(|_| bad("查询词法无效"))?;
    let tokens: Vec<_> = tokens
        .iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect();
    if tokens.windows(2).any(|pair|matches!((pair[0],pair[1]),
        (Token::Word(left),Token::Word(right)) if left.quote_style.is_none()&&right.quote_style.is_none()
            &&left.keyword==Keyword::LIMIT&&right.keyword==Keyword::ALL)) {
        Err(denied("LIMIT ALL尚未支持"))
    } else {Ok(())}
}
