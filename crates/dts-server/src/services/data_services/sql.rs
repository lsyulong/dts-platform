// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    bad,
    types::{ApiMetadata, ApiResponseField},
};
use crate::{
    models::error::AppError,
    services::{product_jobs::parameters, query_svc::validate_read_only_sql},
};
use dts_backend::repo::metadata_catalog::MetadataColumn;
use sqlparser::{
    ast::*,
    dialect::DuckDbDialect,
    keywords::Keyword,
    parser::Parser,
    tokenizer::{Token, Tokenizer},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
};

pub(super) struct CompiledSelect {
    pub sql: String,
    pub schema: Vec<ApiResponseField>,
}

pub(super) fn compile(
    source: &str,
    metadata: &ApiMetadata,
    row_limit: u32,
) -> Result<CompiledSelect, AppError> {
    if source.trim().is_empty() || source.len() > 32768 {
        return Err(bad("数据 API SQL 必须为 1 至 32768 字节"));
    }
    validate_read_only_sql(source)?;
    reject_limit_all(source)?;
    let mut statements = Parser::new(&DuckDbDialect {})
        .with_recursion_limit(64)
        .try_with_sql(source)
        .and_then(|mut parser| parser.parse_statements())
        .map_err(|_| bad("数据 API SQL 无法解析"))?;
    if statements.len() != 1 {
        return Err(bad("数据 API 只允许单条明细 SELECT"));
    }
    let Statement::Query(query) = &mut statements[0] else {
        return Err(bad("数据 API 只允许明细 SELECT"));
    };
    query_shape(query)?;
    let (table, alias) = source_table(query, metadata)?;
    let mut guard = FilterGuard {
        columns: &metadata.columns,
        table: &table,
        alias: alias.as_deref(),
        nodes: 0,
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        return Err(bad("数据 API 不支持集合或子查询"));
    };
    let mut names = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    let mut schema = Vec::new();
    if select.projection.is_empty() || select.projection.len() > 128 {
        return Err(bad("数据 API 必须明确选择 1 至 128 个字段，不支持星号"));
    }
    for item in &mut select.projection {
        let (expr, alias) = match item {
            SelectItem::UnnamedExpr(expr) => (expr, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias)),
            _ => return Err(bad("数据 API 只支持直接字段与唯一别名，不支持星号或聚合")),
        };
        let column = guard.direct(expr)?.clone();
        let name = alias
            .as_ref()
            .map(|alias| alias.value.clone())
            .unwrap_or_else(|| column.name.clone());
        identifier(&name)?;
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(bad("数据 API 返回字段重复，请使用唯一别名"));
        }
        if alias.is_some()
            && metadata
                .columns
                .iter()
                .any(|column| column.name.eq_ignore_ascii_case(&name))
        {
            return Err(bad("返回别名不能遮蔽真实字段，请使用独立名称"));
        }
        schema.push(ApiResponseField {
            name: name.clone(),
            data_type: super::schema::canonical_type(&column.data_type)?,
            nullable: column.nullable,
            description: String::new(),
        });
        *expr = Expr::Identifier(Ident::with_quote('"', &column.name));
        if let Some(alias) = alias {
            *alias = Ident::with_quote('"', &name);
            aliases.insert(name.to_ascii_lowercase(), expr.clone());
        }
    }
    if let Some(selection) = &select.selection {
        guard.check(selection)?;
    }
    if let Some(order) = &query.order_by {
        if order.interpolate.is_some() {
            return Err(bad("数据 API 不支持排序插值"));
        }
        let OrderByKind::Expressions(items) = &order.kind else {
            return Err(bad("数据 API 不支持 ORDER BY ALL"));
        };
        for item in items {
            if item.with_fill.is_some() {
                return Err(bad("数据 API 不支持排序填充"));
            }
            let actual = match &item.expr {
                Expr::Identifier(id) => aliases
                    .get(&id.value.to_ascii_lowercase())
                    .unwrap_or(&item.expr),
                _ => &item.expr,
            };
            guard.direct(actual)?;
        }
    }
    constrain_limit(query, row_limit)?;
    let sql = parameters::render_statement(statements.remove(0))?;
    Ok(CompiledSelect { sql, schema })
}

// LIMIT ALL is normalized away in the parser's Query AST. Token validation
// preserves this rejected source syntax without matching strings or comments.
fn reject_limit_all(source: &str) -> Result<(), AppError> {
    let tokens = Tokenizer::new(&DuckDbDialect {}, source)
        .tokenize()
        .map_err(|_| bad("API SQL token无法解析"))?
        .into_iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect::<Vec<_>>();
    let keyword = |token: &Token, expected| matches!(token,Token::Word(word) if word.quote_style.is_none()&&word.keyword==expected);
    if tokens
        .windows(2)
        .any(|pair| keyword(&pair[0], Keyword::LIMIT) && keyword(&pair[1], Keyword::ALL))
    {
        return Err(bad("数据 API 不支持 LIMIT ALL"));
    }
    Ok(())
}

fn query_shape(query: &Query) -> Result<(), AppError> {
    if query.with.is_some()
        || query.fetch.is_some()
        || query.for_clause.is_some()
        || query.format_clause.is_some()
        || query.settings.is_some()
        || !query.pipe_operators.is_empty()
        || !query.locks.is_empty()
    {
        return Err(bad("数据 API 不支持 CTE、锁或高级查询修饰"));
    }
    let SetExpr::Select(select) = &*query.body else {
        return Err(bad("数据 API 只支持单表明细 SELECT"));
    };
    if select.flavor != SelectFlavor::Standard
        || select.from.len() != 1
        || !select.from[0].joins.is_empty()
        || select.into.is_some()
        || select.top.is_some()
        || select.distinct.is_some()
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
        || !matches!(&select.group_by,GroupByExpr::Expressions(expr,modifiers) if expr.is_empty() && modifiers.is_empty())
    {
        return Err(bad(
            "数据 API 只支持单表直接字段、WHERE、ORDER BY 和有界 LIMIT",
        ));
    }
    Ok(())
}

fn source_table(
    query: &Query,
    metadata: &ApiMetadata,
) -> Result<(Vec<String>, Option<String>), AppError> {
    let SetExpr::Select(select) = &*query.body else {
        return Err(bad("数据 API 缺少单表查询"));
    };
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
        return Err(bad("数据 API 禁止外部表函数、派生表或其他来源"));
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
        return Err(bad("数据 API 不支持表版本、采样或来源字段重命名"));
    }
    let table = name
        .0
        .iter()
        .map(|part| {
            part.as_ident()
                .map(|id| id.value.clone())
                .ok_or_else(|| bad("实际表标识无效"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let binding = &metadata.binding;
    let expected = [
        &binding.table_catalog,
        &binding.schema_name,
        &binding.table_name,
    ];
    if table.len() != 3
        || table
            .iter()
            .zip(expected)
            .any(|(name, expected)| !name.eq_ignore_ascii_case(expected))
    {
        return Err(bad("API SQL 必须唯一读取绑定的真实 catalog.schema.table"));
    }
    for name in &table {
        identifier(name)?;
    }
    Ok((table, alias.as_ref().map(|alias| alias.name.value.clone())))
}

pub(super) fn identifier(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty()
        || name.len() > 256
        || name.chars().any(char::is_control)
        || name.contains(['"', '\\'])
    {
        return Err(bad("数据 API 标识为空、过长或含不支持的引号/反斜线"));
    }
    Ok(())
}

fn literal_limit(expr: &Expr, max: u64) -> Result<u64, AppError> {
    match expr {
        Expr::Value(value) => match &value.value {
            Value::Number(text, false)
                if !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit()) =>
            {
                text.parse::<u64>()
                    .ok()
                    .filter(|value| *value <= max)
                    .ok_or_else(|| bad("LIMIT/OFFSET 超过范围"))
            },
            _ => Err(bad("LIMIT/OFFSET 必须为有界整数")),
        },
        _ => Err(bad("LIMIT/OFFSET 不允许参数或表达式")),
    }
}

fn constrain_limit(query: &mut Query, row_limit: u32) -> Result<(), AppError> {
    let maximum = u64::from(row_limit) + 1;
    let (limit, offset) = match query.limit_clause.take() {
        None => (maximum, None),
        Some(LimitClause::LimitOffset {
            limit: Some(limit),
            offset,
            limit_by,
        }) if limit_by.is_empty() => {
            if let Some(offset) = &offset {
                literal_limit(&offset.value, 1_000_000)?;
            }
            (literal_limit(&limit, 10000)?.min(maximum), offset)
        },
        _ => return Err(bad("数据 API 不支持 LIMIT ALL、LIMIT BY 或逗号分页")),
    };
    query.limit_clause = Some(LimitClause::LimitOffset {
        limit: Some(Expr::Value(Value::Number(limit.to_string(), false).into())),
        offset,
        limit_by: vec![],
    });
    Ok(())
}

struct FilterGuard<'a> {
    columns: &'a [MetadataColumn],
    table: &'a [String],
    alias: Option<&'a str>,
    nodes: usize,
}
impl<'a> FilterGuard<'a> {
    fn direct(&self, expr: &Expr) -> Result<&'a MetadataColumn, AppError> {
        let (name, prefix) = match expr {
            Expr::Identifier(id) => (&id.value, vec![]),
            Expr::CompoundIdentifier(parts) => {
                let (name, prefix) = parts.split_last().ok_or_else(|| bad("字段引用为空"))?;
                (
                    &name.value,
                    prefix.iter().map(|id| id.value.clone()).collect(),
                )
            },
            _ => return Err(bad("数据 API 只允许明确的真实字段引用")),
        };
        if !prefix.is_empty() {
            let same = |expected: &[String]| {
                prefix.len() == expected.len()
                    && prefix
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
            };
            if !(same(self.table)
                || same(&self.table[1..])
                || same(&self.table[2..])
                || prefix.len() == 1
                    && self
                        .alias
                        .is_some_and(|alias| alias.eq_ignore_ascii_case(&prefix[0])))
            {
                return Err(bad("字段限定名不属于 API 的实际表"));
            }
        }
        self.columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| bad("SQL 使用了未实际采集的字段"))
    }
    fn check(&mut self, expr: &Expr) -> Result<(), AppError> {
        match expr.visit(self) {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(reason) => Err(bad(reason)),
        }
    }
}
impl Visitor for FilterGuard<'_> {
    type Break = &'static str;
    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        self.nodes += 1;
        if self.nodes > 2000 {
            return ControlFlow::Break("WHERE 条件超过复杂度上限");
        }
        let valid = match expr {
            Expr::Identifier(_) | Expr::CompoundIdentifier(_) => self.direct(expr).is_ok(),
            Expr::BinaryOp { op, .. } => matches!(
                op,
                BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::And
                    | BinaryOperator::Or
            ),
            Expr::UnaryOp { op, .. } => matches!(
                op,
                UnaryOperator::Not | UnaryOperator::Plus | UnaryOperator::Minus
            ),
            Expr::Value(value) => matches!(
                &value.value,
                Value::Number(_, _)
                    | Value::SingleQuotedString(_)
                    | Value::Boolean(_)
                    | Value::Null
                    | Value::Placeholder(_)
            ),
            Expr::TypedString(value) => {
                matches!(value.data_type.to_string().as_str(), "DATE" | "TIMESTAMP")
            },
            Expr::Nested(_)
            | Expr::IsNull(_)
            | Expr::IsNotNull(_)
            | Expr::IsTrue(_)
            | Expr::IsFalse(_)
            | Expr::IsDistinctFrom(_, _)
            | Expr::IsNotDistinctFrom(_, _)
            | Expr::InList { .. }
            | Expr::Between { .. } => true,
            _ => false,
        };
        if valid {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break("WHERE 含未知字段、函数、子查询或不支持的表达式")
        }
    }
}
