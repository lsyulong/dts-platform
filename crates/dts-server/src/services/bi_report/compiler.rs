// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, definition, types::*};
use crate::{models::error::AppError, services::query_svc::validate_read_only_sql};
use dts_backend::repo::{
    metadata_catalog::binding_matches,
    product_job::{ProductJob, ProductJobNodeDefinition},
};
use serde_json::json;
use sqlparser::{ast::*, dialect::DuckDbDialect, parser::Parser};

fn query_ast(template: &str) -> Result<Box<Query>, AppError> {
    let mut statements = Parser::parse_sql(&DuckDbDialect {}, template)
        .map_err(|_| bad("内部 BI SQL 模板无法解析"))?;
    if statements.len() != 1 {
        return Err(bad("内部 BI 查询不是单条语句"));
    }
    match statements.remove(0) {
        Statement::Query(query) => Ok(query),
        _ => Err(bad("内部 BI 模板不是 SELECT")),
    }
}
fn select(query: &mut Query) -> Result<&mut Select, AppError> {
    match query.body.as_mut() {
        SetExpr::Select(select) => Ok(select),
        _ => Err(bad("内部 BI 模板不是单表 SELECT")),
    }
}
fn id(name: &str) -> Result<Ident, AppError> {
    definition::identifier(name)?;
    Ok(Ident::with_quote('"', name))
}
fn expr(name: &str) -> Result<Expr, AppError> {
    Ok(Expr::Identifier(id(name)?))
}

fn measure(measure: &ReportMeasure) -> Result<SelectItem, AppError> {
    let template = match measure.aggregate {
        Aggregate::Sum => "SELECT SUM(value) AS metric",
        Aggregate::Avg => "SELECT AVG(value) AS metric",
        Aggregate::Count => "SELECT COUNT(value) AS metric",
        Aggregate::Min => "SELECT MIN(value) AS metric",
        Aggregate::Max => "SELECT MAX(value) AS metric",
    };
    let mut query = query_ast(template)?;
    let SelectItem::ExprWithAlias {
        expr: Expr::Function(function),
        alias,
    } = select(&mut query)?
        .projection
        .first_mut()
        .ok_or_else(|| bad("内部聚合投影缺失"))?
    else {
        return Err(bad("内部聚合结构无效"));
    };
    let FunctionArguments::List(arguments) = &mut function.args else {
        return Err(bad("内部聚合参数结构无效"));
    };
    arguments.args = vec![FunctionArg::Unnamed(
        if measure.aggregate == Aggregate::Count && measure.expression == "*" {
            FunctionArgExpr::Wildcard
        } else {
            FunctionArgExpr::Expr(expr(&measure.expression)?)
        },
    )];
    *alias = id(&measure.name)?;
    Ok(select(&mut query)?.projection.remove(0))
}

pub(super) fn sql(snapshot: &ReportSnapshot) -> Result<String, AppError> {
    let binding = &snapshot.metadata.binding;
    let mut query =
        query_ast("SELECT value FROM placeholder GROUP BY value ORDER BY value LIMIT 1")?;
    let dimensions = snapshot
        .report
        .dimensions
        .iter()
        .map(|name| expr(name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut projection = dimensions
        .iter()
        .cloned()
        .map(SelectItem::UnnamedExpr)
        .collect::<Vec<_>>();
    projection.extend(
        snapshot
            .report
            .measures
            .iter()
            .map(measure)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let select = select(&mut query)?;
    let TableFactor::Table { name, .. } = &mut select.from[0].relation else {
        return Err(bad("内部报表不是单一真实表"));
    };
    *name = ObjectName(
        [
            &binding.table_catalog,
            &binding.schema_name,
            &binding.table_name,
        ]
        .into_iter()
        .map(|name| id(name).map(ObjectNamePart::Identifier))
        .collect::<Result<Vec<_>, _>>()?,
    );
    select.projection = projection;
    select.group_by = GroupByExpr::Expressions(dimensions.clone(), vec![]);
    if dimensions.is_empty() {
        query.order_by = None;
    } else {
        let order = query
            .order_by
            .as_mut()
            .ok_or_else(|| bad("内部排序模板缺失"))?;
        let OrderByKind::Expressions(items) = &mut order.kind else {
            return Err(bad("内部排序模板无效"));
        };
        let prototype = items
            .first()
            .cloned()
            .ok_or_else(|| bad("内部排序字段缺失"))?;
        *items = dimensions
            .into_iter()
            .map(|expr| OrderByExpr {
                expr,
                ..prototype.clone()
            })
            .collect();
    }
    match query.limit_clause.as_mut() {
        Some(LimitClause::LimitOffset { limit, .. }) => {
            *limit = Some(Expr::Value(
                sqlparser::ast::Value::Number((snapshot.report.row_limit + 1).to_string(), false)
                    .into(),
            ))
        },
        _ => return Err(bad("内部报表结果限制无效")),
    }
    let sql = query.to_string();
    validate_read_only_sql(&sql)?;
    Ok(sql)
}

pub(super) fn node(snapshot: &ReportSnapshot) -> ProductJobNodeDefinition {
    ProductJobNodeDefinition {
        id: "report".into(),
        name: "执行真实 BI 聚合报表".into(),
        definition: json!({"format_version":1,"type":"bi_aggregation","metadata_id":snapshot.metadata.id,
        "metadata_version":snapshot.metadata.version,"report":snapshot.report,"binding":snapshot.metadata.binding}),
        depends_on: vec![],
    }
}

pub(super) fn snapshot(job: &ProductJob) -> Result<ReportSnapshot, AppError> {
    let frozen: ReportSnapshot = serde_json::from_value(job.definition_snapshot.clone())
        .map_err(|_| bad("BI 运行固定快照不符合合同"))?;
    if !job.can_retry
        || job.action != "execute_bi"
        || job.source_kind != "bi_report"
        || frozen.format_version != 1
        || frozen.executor != "bi_aggregate_v1"
        || frozen.tenant_id != job.tenant_id
        || frozen.source_kind != job.source_kind
        || frozen.source_id != job.source_id
        || frozen.source_version != job.source_version
        || job.parameters != json!({})
        || frozen.metadata.binding.engine != "swanlake"
        || frozen.metadata.binding.tenant_id != job.tenant_id
        || frozen.metadata.version != frozen.metadata.binding.object_version
        || frozen.properties["metadata_object_id"] != frozen.metadata.id
        || !binding_matches(&frozen.metadata.binding, &frozen.metadata.properties)
        || frozen.metadata.properties["columns"] != json!(frozen.metadata.columns)
    {
        return Err(bad("BI 运行、真实数据源身份与固定快照不一致"));
    }
    let report = definition::parse(
        &frozen.properties,
        &frozen.metadata.columns,
        &frozen.report.chart_config.title,
    )?;
    if report != frozen.report {
        return Err(bad("BI 受控报表定义与固定快照不一致"));
    }
    Ok(frozen)
}

pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let frozen = snapshot(job)?;
    let expected = node(&frozen);
    if job.nodes.len() != 1
        || job.nodes[0].id != expected.id
        || job.nodes[0].name != expected.name
        || job.nodes[0].definition != expected.definition
        || job.nodes[0].depends_on != expected.depends_on
    {
        return Err(bad("BI 节点不符合固定报表定义"));
    }
    Ok(vec![(expected.id, sql(&frozen)?)])
}
