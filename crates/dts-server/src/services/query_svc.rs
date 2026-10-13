// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::time::Duration;

use arrow_flight::flight_service_client::FlightServiceClient;
use arrow_flight::sql::client::FlightSqlServiceClient;
use arrow_schema::SchemaRef;
use futures::StreamExt;
use serde::Serialize;
use sqlparser::ast::{Expr, Query, SetExpr, Statement, TableFactor, Visit, Visitor};
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;
use tonic::transport::{Channel, Endpoint};
use tracing::{info, warn};

use crate::config::SwanLakeConfig;
use crate::models::error::AppError;

mod arrow_json;

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
    pub suggested_chart: Option<ChartSuggestion>,
}

#[derive(Debug, Serialize)]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
    pub is_numeric: bool,
}

#[derive(Debug, Serialize)]
pub struct ChartSuggestion {
    pub chart_type: String,
    pub x_column: Option<String>,
    pub y_column: Option<String>,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SchemaInfo {
    pub catalog: Option<String>,
    pub schema: String,
    pub tables: Vec<TableInfo>,
}

#[derive(Debug, Serialize)]
pub struct TableInfo {
    pub name: String,
}

#[derive(Clone, Copy)]
struct QueryLimits {
    max_rows: usize,
    max_json_bytes: usize,
    max_batch_bytes: usize,
    timeout: Duration,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_rows: 10_000,
            max_json_bytes: 8 * 1024 * 1024,
            max_batch_bytes: 32 * 1024 * 1024,
            timeout: Duration::from_secs(60),
        }
    }
}

/// Executes bounded read-only queries against the tenant's SwanLake endpoint.
#[derive(Clone)]
pub struct QueryService {
    endpoint: String,
    enabled: bool,
    limits: QueryLimits,
}

impl QueryService {
    pub fn new(config: &SwanLakeConfig) -> Self {
        Self {
            endpoint: grpc_to_http_endpoint(&config.uri()),
            enabled: config.enabled,
            limits: QueryLimits::default(),
        }
    }

    pub(crate) async fn connect(&self) -> Result<FlightSqlServiceClient<Channel>, AppError> {
        let channel = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|_| AppError::Internal("SwanLake 地址配置无效".into()))?
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .map_err(|_| AppError::Internal("SwanLake 连接失败，请检查分析引擎状态".into()))?;

        let flight = FlightServiceClient::new(channel).max_decoding_message_size(16 * 1024 * 1024);
        Ok(FlightSqlServiceClient::new_from_inner(flight))
    }

    pub async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        if !self.enabled {
            return Err(AppError::BadRequest("SwanLake 分析引擎未启用".into()));
        }
        validate_read_only_sql(sql)?;
        tokio::time::timeout(self.limits.timeout, self.execute_inner(sql))
            .await
            .map_err(|_| AppError::BadRequest("查询超时，请缩小查询范围或减少结果行数".into()))?
    }

    async fn execute_inner(&self, sql: &str) -> Result<QueryResult, AppError> {
        let mut client = self.connect().await?;
        let flight_info = client
            .execute(sql.to_string(), None)
            .await
            .map_err(|error| {
                if let arrow_flight::error::FlightError::Tonic(status) = &error {
                    warn!(code = ?status.code(), "SwanLake 查询请求失败");
                } else {
                    warn!("SwanLake 查询请求协议错误");
                }
                AppError::BadRequest("查询执行失败，请检查 SQL 与分析引擎状态".into())
            })?;
        let schema: SchemaRef = std::sync::Arc::new(
            flight_info
                .clone()
                .try_decode_schema()
                .map_err(|_| AppError::Internal("分析引擎返回了无效的结果结构".into()))?,
        );
        let mut collector = arrow_json::ResultCollector::new(schema, self.limits)?;

        for endpoint in flight_info.endpoint {
            // SwanLake serves its result tickets on the established connection.
            // Refuse redirects to arbitrary Flight servers.
            if !endpoint.location.is_empty() {
                return Err(AppError::Internal("分析引擎返回了不支持的结果端点".into()));
            }
            let ticket = endpoint
                .ticket
                .ok_or_else(|| AppError::Internal("分析引擎返回的结果缺少凭证".into()))?;
            let mut stream = client
                .do_get(ticket)
                .await
                .map_err(|_| AppError::Internal("获取查询结果失败，请检查分析引擎状态".into()))?;
            while let Some(batch_result) = stream.next().await {
                let batch = batch_result
                    .map_err(|_| AppError::Internal("查询结果数据流读取失败".into()))?;
                collector.append(&batch)?;
                tokio::task::yield_now().await;
            }
        }

        let (columns, rows) = collector.into_parts();
        let row_count = rows.len();
        let suggested_chart = suggest_chart(&columns);
        info!(row_count, columns = columns.len(), "SwanLake 查询完成");
        Ok(QueryResult {
            columns,
            rows,
            row_count,
            suggested_chart,
        })
    }

    pub async fn list_schemas(&self) -> Result<Vec<SchemaInfo>, AppError> {
        let result = self
            .execute(concat!(
                "SELECT table_catalog, table_schema, table_name ",
                "FROM information_schema.tables ",
                "ORDER BY table_catalog, table_schema, table_name"
            ))
            .await?;
        let mut schemas: BTreeMap<(String, String), Vec<TableInfo>> = BTreeMap::new();
        for row in result.rows {
            let field = |name: &str| {
                row.get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| AppError::Internal("分析引擎返回了无效的表目录".into()))
            };
            let catalog = field("table_catalog")?;
            let schema = field("table_schema")?;
            let table = field("table_name")?;
            schemas
                .entry((catalog, schema))
                .or_default()
                .push(TableInfo { name: table });
        }
        Ok(schemas
            .into_iter()
            .map(|((catalog, schema), tables)| SchemaInfo {
                catalog: Some(catalog),
                schema,
                tables,
            })
            .collect())
    }
}

fn grpc_to_http_endpoint(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("grpc://") {
        format!("http://{rest}")
    } else if let Some(rest) = uri.strip_prefix("grpc+tls://") {
        format!("https://{rest}")
    } else {
        uri.to_string()
    }
}

pub(crate) fn validate_read_only_sql(sql: &str) -> Result<(), AppError> {
    if sql.len() > 64 * 1024 {
        return Err(AppError::BadRequest("SQL 长度超过 64 KiB 限制".into()));
    }
    let statements = Parser::parse_sql(&DuckDbDialect {}, sql)
        .map_err(|_| AppError::BadRequest("SQL 语法无法识别，请提交单条只读查询".into()))?;
    if statements.len() != 1 {
        return Err(AppError::BadRequest("仅支持单条只读查询".into()));
    }
    match statements.visit(&mut ReadOnlyQuery) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(reason) => Err(AppError::BadRequest(reason.into())),
    }
}

// This narrows the SQL surface; server filesystem/network isolation and tenant
// endpoint ownership remain necessary. It cannot sandbox custom SQL functions.
struct ReadOnlyQuery;

impl Visitor for ReadOnlyQuery {
    type Break = &'static str;

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
        if matches!(statement, Statement::Query(_)) {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break("查询入口仅支持只读 SELECT 或 VALUES")
        }
    }

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if !query.locks.is_empty() || query.settings.is_some() || !query.pipe_operators.is_empty() {
            return ControlFlow::Break("查询入口不支持锁、设置或管道操作");
        }
        validate_query_body(&query.body)
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        match factor {
            TableFactor::Table {
                name, args: None, ..
            } => {
                let display_name = name.to_string();
                if display_name.contains(['/', '\\'])
                    || display_name.contains("://")
                    || name.0.iter().any(|part| {
                        part.as_ident()
                            .is_none_or(|ident| ident.value.contains('.'))
                    })
                {
                    ControlFlow::Break("查询入口不支持直接读取外部文件")
                } else {
                    ControlFlow::Continue(())
                }
            },
            TableFactor::Derived { .. }
            | TableFactor::NestedJoin { .. }
            | TableFactor::UNNEST { .. } => ControlFlow::Continue(()),
            _ => ControlFlow::Break("查询入口不支持外部表函数，请先将数据接入分析表"),
        }
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        if let Expr::Function(function) = expr {
            let name = function
                .name
                .to_string()
                .to_ascii_lowercase()
                .replace('"', "");
            let name = name.rsplit('.').next().unwrap_or(&name);
            if name.starts_with("read_")
                || name.starts_with("write_")
                || name.ends_with("_scan")
                || matches!(
                    name,
                    "query"
                        | "query_table"
                        | "getenv"
                        | "nextval"
                        | "setval"
                        | "load_extension"
                        | "http_get"
                        | "http_post"
                )
            {
                return ControlFlow::Break("查询入口不支持读取外部资源或有副作用的函数");
            }
        }
        ControlFlow::Continue(())
    }
}

fn validate_query_body(body: &SetExpr) -> ControlFlow<&'static str> {
    match body {
        SetExpr::Select(select) if select.into.is_none() => ControlFlow::Continue(()),
        SetExpr::Query(_) | SetExpr::Values(_) => ControlFlow::Continue(()),
        SetExpr::SetOperation { left, right, .. } => {
            validate_query_body(left)?;
            validate_query_body(right)
        },
        _ => ControlFlow::Break("查询入口不支持写入或 SELECT INTO"),
    }
}

fn suggest_chart(columns: &[ColumnMeta]) -> Option<ChartSuggestion> {
    let numeric = columns.iter().find(|column| column.is_numeric)?;
    let time = columns.iter().find(|column| {
        column.data_type.starts_with("Date") || column.data_type.starts_with("Timestamp")
    });
    if let Some(time) = time {
        return Some(ChartSuggestion {
            chart_type: "line".into(),
            x_column: Some(time.name.clone()),
            y_column: Some(numeric.name.clone()),
            reason: format!(
                "时间列 '{}' 与数值列 '{}' 适合展示趋势",
                time.name, numeric.name
            ),
        });
    }
    let category = columns
        .iter()
        .find(|column| column.data_type.contains("Utf8"));
    Some(ChartSuggestion {
        chart_type: "bar".into(),
        x_column: category.map(|column| column.name.clone()),
        y_column: Some(numeric.name.clone()),
        reason: format!("数值列 '{}' 适合柱状图对比", numeric.name),
    })
}

#[cfg(test)]
mod tests;
