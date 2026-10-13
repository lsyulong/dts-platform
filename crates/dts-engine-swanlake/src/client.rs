// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use arrow_array::{
    Array, Int64Array, LargeStringArray, RecordBatch, StringArray, StringViewArray, UInt64Array,
};
use arrow_flight::flight_service_client::FlightServiceClient;
use arrow_flight::sql::client::FlightSqlServiceClient;
use arrow_flight::sql::{CommandStatementUpdate, DoPutUpdateResult, ProstMessageExt};
use arrow_flight::{FlightData, FlightDescriptor};
use dts_engine::EngineError;
use futures::StreamExt;
use prost::Message;
use tonic::transport::{Channel, Endpoint};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) struct SwanLakeClient {
    inner: FlightSqlServiceClient<Channel>,
}

impl SwanLakeClient {
    pub(crate) async fn connect(endpoint: String) -> Result<Self, EngineError> {
        let channel = Endpoint::from_shared(endpoint)
            .map_err(|_| EngineError::InvalidConfig("SwanLake 地址无效".into()))?
            .connect_timeout(CONNECT_TIMEOUT)
            .connect()
            .await
            .map_err(|_| EngineError::TargetConnection("无法连接 SwanLake".into()))?;
        let flight = FlightServiceClient::new(channel).max_decoding_message_size(16 * 1024 * 1024);
        Ok(Self {
            inner: FlightSqlServiceClient::new_from_inner(flight),
        })
    }

    pub(crate) async fn update(&mut self, sql: String) -> Result<i64, EngineError> {
        let command = CommandStatementUpdate {
            query: sql,
            transaction_id: None,
        };
        let descriptor = FlightDescriptor::new_cmd(command.as_any().encode_to_vec());
        let stream = futures::stream::iter([FlightData {
            flight_descriptor: Some(descriptor),
            ..Default::default()
        }]);
        let operation = async {
            let mut response = self
                .inner
                .inner_mut()
                .do_put(stream)
                .await
                .map_err(|_| EngineError::Internal("SwanLake 更新请求失败".into()))?
                .into_inner();
            let metadata = response
                .message()
                .await
                .map_err(|_| EngineError::Internal("SwanLake 更新响应中断".into()))?
                .ok_or_else(|| EngineError::Internal("SwanLake 未返回更新结果".into()))?;
            DoPutUpdateResult::decode(metadata.app_metadata)
                .map(|result| result.record_count)
                .map_err(|_| EngineError::Internal("SwanLake 更新响应无效".into()))
        };
        tokio::time::timeout(OPERATION_TIMEOUT, operation)
            .await
            .map_err(|_| EngineError::Internal("SwanLake 更新执行超时".into()))?
    }

    pub(crate) async fn scalar_u64(&mut self, sql: String) -> Result<u64, EngineError> {
        for batch in self.query_batches(sql).await? {
            if batch.num_rows() == 0 || batch.num_columns() != 1 {
                continue;
            }
            let column = batch.column(0);
            let value = if let Some(values) = column.as_any().downcast_ref::<Int64Array>() {
                u64::try_from(values.value(0)).ok()
            } else {
                column
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .map(|values| values.value(0))
            };
            if let Some(value) = value {
                return Ok(value);
            }
        }
        Err(EngineError::Internal("SwanLake 未返回有效计数".into()))
    }

    pub(crate) async fn string_pairs(
        &mut self,
        sql: String,
    ) -> Result<Vec<(String, String)>, EngineError> {
        let mut result = Vec::new();
        for batch in self.query_batches(sql).await? {
            if batch.num_columns() != 2 {
                return Err(EngineError::Internal("SwanLake 目录响应结构无效".into()));
            }
            for row in 0..batch.num_rows() {
                let left = string_value(batch.column(0), row)
                    .ok_or_else(|| EngineError::Internal("SwanLake 目录字段无效".into()))?;
                let right = string_value(batch.column(1), row)
                    .ok_or_else(|| EngineError::Internal("SwanLake 目录字段无效".into()))?;
                result.push((left, right));
            }
        }
        Ok(result)
    }

    async fn query_batches(&mut self, sql: String) -> Result<Vec<RecordBatch>, EngineError> {
        let operation = async {
            let info = self
                .inner
                .execute(sql, None)
                .await
                .map_err(|_| EngineError::Internal("SwanLake 查询请求失败".into()))?;
            let mut batches = Vec::new();
            for endpoint in info.endpoint {
                if !endpoint.location.is_empty() {
                    return Err(EngineError::Internal(
                        "SwanLake 返回了不受信任的重定向端点".into(),
                    ));
                }
                let ticket = endpoint
                    .ticket
                    .ok_or_else(|| EngineError::Internal("SwanLake 查询缺少凭证".into()))?;
                let mut stream = self
                    .inner
                    .do_get(ticket)
                    .await
                    .map_err(|_| EngineError::Internal("SwanLake 结果读取失败".into()))?;
                while let Some(batch) = stream.next().await {
                    batches.push(
                        batch.map_err(|_| EngineError::Internal("SwanLake 结果流中断".into()))?,
                    );
                }
            }
            Ok(batches)
        };
        tokio::time::timeout(OPERATION_TIMEOUT, operation)
            .await
            .map_err(|_| EngineError::Internal("SwanLake 查询执行超时".into()))?
    }
}

fn string_value(array: &arrow_array::ArrayRef, row: usize) -> Option<String> {
    if array.is_null(row) {
        return None;
    }
    if let Some(values) = array.as_any().downcast_ref::<StringArray>() {
        return Some(values.value(row).to_owned());
    }
    if let Some(values) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Some(values.value(row).to_owned());
    }
    array
        .as_any()
        .downcast_ref::<StringViewArray>()
        .map(|values| values.value(row).to_owned())
}
