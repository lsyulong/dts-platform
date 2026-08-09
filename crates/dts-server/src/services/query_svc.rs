// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


use arrow_array::RecordBatch;
use arrow_flight::sql::client::FlightSqlServiceClient;
use arrow_schema::SchemaRef;
use futures::StreamExt;
use serde::Serialize;
use tonic::transport::{Channel, Endpoint};
use tracing::info;

use crate::config::SwanLakeConfig;
use crate::models::error::AppError;

/// 查询结果 — JSON 行格式
#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
}

/// Schema/表信息
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

/// SwanLake 查询服务
///
/// 通过 Arrow Flight SQL 协议连接 SwanLake (DuckDB) 执行分析查询。
#[derive(Clone)]
pub struct QueryService {
    /// gRPC endpoint (http://host:port — tonic 格式)
    endpoint: String,
    enabled: bool,
}

impl QueryService {
    pub fn new(config: &SwanLakeConfig) -> Self {
        Self {
            endpoint: grpc_to_http_endpoint(&config.uri()),
            enabled: config.enabled,
        }
    }

    /// 建立 Flight SQL 连接
    async fn connect(&self) -> Result<FlightSqlServiceClient<Channel>, AppError> {
        let channel = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| {
                AppError::Internal(format!("无效的 SwanLake 地址 '{}': {e}", self.endpoint))
            })?
            .connect()
            .await
            .map_err(|e| AppError::Internal(format!("SwanLake 连接失败: {e}")))?;

        Ok(FlightSqlServiceClient::new(channel))
    }

    /// 执行 SQL 查询并返回 JSON 结果
    pub async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        if !self.enabled {
            return Err(AppError::BadRequest("SwanLake 分析引擎未启用".into()));
        }

        let mut client = self.connect().await?;
        info!(sql = %sql, "执行 SwanLake 查询");

        let flight_info = client
            .execute(sql.to_string(), None)
            .await
            .map_err(|e| AppError::BadRequest(format!("查询执行失败: {e}")))?;

        let mut columns: Vec<ColumnMeta> = Vec::new();
        let mut all_rows: Vec<serde_json::Value> = Vec::new();

        for endpoint in &flight_info.endpoint {
            if let Some(ref ticket) = endpoint.ticket {
                let mut stream = client
                    .do_get(ticket.clone())
                    .await
                    .map_err(|e| AppError::Internal(format!("获取查询结果失败: {e}")))?;

                // do_get 直接返回 RecordBatch 流
                while let Some(batch_result) = stream.next().await {
                    let batch = batch_result.map_err(|e| {
                        AppError::Internal(format!("读取数据流失败: {e}"))
                    })?;

                    // 从第一个 batch 提取列元数据
                    if columns.is_empty() {
                        columns = columns_from_schema(batch.schema());
                    }

                    let json_rows = record_batch_to_json_rows(&batch)?;
                    all_rows.extend(json_rows);
                }
            }
        }

        let row_count = all_rows.len();
        info!(row_count = row_count, columns = columns.len(), "SwanLake 查询完成");

        Ok(QueryResult {
            columns,
            rows: all_rows,
            row_count,
        })
    }

    /// 列出所有 schema 及其表
    pub async fn list_schemas(&self) -> Result<Vec<SchemaInfo>, AppError> {
        if !self.enabled {
            return Err(AppError::BadRequest("SwanLake 分析引擎未启用".into()));
        }

        let mut client = self.connect().await?;
        info!("获取 SwanLake schema 列表");

        // 使用 DuckDB information_schema 查询
        let sql = concat!(
            "SELECT table_schema, table_name ",
            "FROM information_schema.tables ",
            "ORDER BY table_schema, table_name"
        );
        let (_columns, rows) = self.execute_query_rows(&mut client, sql).await?;

        // 按 schema 分组
        let mut schema_map: std::collections::BTreeMap<String, Vec<TableInfo>> =
            std::collections::BTreeMap::new();

        for row in &rows {
            let schema = row["table_schema"].as_str().unwrap_or("unknown").to_string();
            let table = row["table_name"].as_str().unwrap_or("unknown").to_string();
            schema_map
                .entry(schema)
                .or_default()
                .push(TableInfo { name: table });
        }

        let schemas: Vec<SchemaInfo> = schema_map
            .into_iter()
            .map(|(name, tables)| SchemaInfo {
                catalog: None,
                schema: name,
                tables,
            })
            .collect();

        Ok(schemas)
    }

    /// 底层查询 — 返回 JSON 行
    async fn execute_query_rows(
        &self,
        client: &mut FlightSqlServiceClient<Channel>,
        sql: &str,
    ) -> Result<(Vec<ColumnMeta>, Vec<serde_json::Value>), AppError> {
        let flight_info = client
            .execute(sql.to_string(), None)
            .await
            .map_err(|e| AppError::BadRequest(format!("查询执行失败: {e}")))?;

        let mut columns: Vec<ColumnMeta> = Vec::new();
        let mut all_rows: Vec<serde_json::Value> = Vec::new();

        for endpoint in &flight_info.endpoint {
            if let Some(ref ticket) = endpoint.ticket {
                let mut stream = client
                    .do_get(ticket.clone())
                    .await
                    .map_err(|e| AppError::Internal(format!("获取查询结果失败: {e}")))?;

                while let Some(batch_result) = stream.next().await {
                    let batch = batch_result.map_err(|e| {
                        AppError::Internal(format!("读取数据流失败: {e}"))
                    })?;

                    if columns.is_empty() {
                        columns = columns_from_schema(batch.schema());
                    }

                    let json_rows = record_batch_to_json_rows(&batch)?;
                    all_rows.extend(json_rows);
                }
            }
        }

        Ok((columns, all_rows))
    }
}

/// 将 grpc:// URI 转为 tonic 需要的 http:// URI
fn grpc_to_http_endpoint(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("grpc://") {
        format!("http://{rest}")
    } else if let Some(rest) = uri.strip_prefix("grpc+tls://") {
        format!("https://{rest}")
    } else {
        uri.to_string()
    }
}

/// 从 Arrow Schema 提取列元数据
fn columns_from_schema(schema: SchemaRef) -> Vec<ColumnMeta> {
    schema
        .fields()
        .iter()
        .map(|f| ColumnMeta {
            name: f.name().to_string(),
            data_type: format!("{:?}", f.data_type()),
        })
        .collect()
}

/// 将 Arrow RecordBatch 转为 JSON 行向量
fn record_batch_to_json_rows(
    batch: &RecordBatch,
) -> Result<Vec<serde_json::Value>, AppError> {
    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let num_cols = batch.num_columns();
    let mut rows: Vec<serde_json::Value> = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut map = serde_json::Map::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let col_name = schema.field(col_idx).name().to_string();
            let value = arrow_value_to_json(batch.column(col_idx), row_idx);
            map.insert(col_name, value);
        }
        rows.push(serde_json::Value::Object(map));
    }

    Ok(rows)
}

/// 将 Arrow 数组中的单个值转为 serde_json::Value
fn arrow_value_to_json(array: &dyn arrow_array::Array, idx: usize) -> serde_json::Value {
    use arrow_array::*;

    if array.is_null(idx) {
        return serde_json::Value::Null;
    }

    // 按常见类型逐一尝试 downcast
    macro_rules! downcast_num {
        ($arr:expr, $idx:expr, $t:ty) => {{
            if let Some(a) = $arr.as_any().downcast_ref::<$t>() {
                return serde_json::json!(a.value($idx));
            }
        }};
    }

    downcast_num!(array, idx, Int64Array);
    downcast_num!(array, idx, Int32Array);
    downcast_num!(array, idx, Int16Array);
    downcast_num!(array, idx, Int8Array);
    downcast_num!(array, idx, UInt64Array);
    downcast_num!(array, idx, UInt32Array);
    downcast_num!(array, idx, UInt16Array);
    downcast_num!(array, idx, UInt8Array);
    downcast_num!(array, idx, Float64Array);
    downcast_num!(array, idx, Float32Array);

    if let Some(arr) = array.as_any().downcast_ref::<StringArray>() {
        return serde_json::Value::String(arr.value(idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<LargeStringArray>() {
        return serde_json::Value::String(arr.value(idx).to_string());
    }
    if let Some(arr) = array.as_any().downcast_ref::<BooleanArray>() {
        return serde_json::Value::Bool(arr.value(idx));
    }

    // Fallback: 返回 null（未知类型）
    serde_json::Value::Null
}
