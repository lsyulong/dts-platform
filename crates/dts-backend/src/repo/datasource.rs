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

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::BackendError;
use crate::types::{CreateDatasourceParams, DatasourceDetail, DatasourceInfo};

/// 引擎所需的原始连接信息（含密码，不外泄到 API）
#[derive(Clone)]
pub struct RawConnectionInfo {
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database_name: Option<String>,
}

impl std::fmt::Debug for RawConnectionInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RawConnectionInfo")
            .field("db_type", &self.db_type)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("database_name", &self.database_name)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceProbe {
    pub id: String,
    pub datasource_id: String,
    pub supported: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub actor_id: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecordDatasourceProbeParams {
    pub id: String,
    pub tenant_id: String,
    pub datasource_id: String,
    pub supported: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
    pub actor_id: String,
    pub observed_at: DateTime<Utc>,
}

/// 数据源数据访问 trait
#[async_trait]
pub trait DatasourceRepo: Send + Sync {
    /// 创建数据源
    async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError>;

    /// 获取租户下的所有数据源
    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<DatasourceInfo>, BackendError>;

    /// 获取单个数据源详情（密码脱敏）
    async fn find_detail_by_id(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<DatasourceDetail>, BackendError>;

    /// 获取引擎所需的原始连接信息（含真实密码，仅供内部引擎使用）
    async fn get_connection_info(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<RawConnectionInfo>, BackendError>;

    /// 删除数据源
    async fn delete(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError>;

    /// 统计租户数据源数量
    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError>;

    /// 统计租户健康数据源数量
    async fn count_healthy_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError>;

    /// 检查数据源是否存在且属于该租户
    async fn exists(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError>;

    /// Persist one probe result and update the datasource's current health atomically.
    async fn record_probe(
        &self,
        params: &RecordDatasourceProbeParams,
    ) -> Result<DatasourceProbe, BackendError> {
        let _ = params;
        Err(BackendError::Configuration(
            "当前存储后端不支持数据源探测记录".into(),
        ))
    }

    /// Return newest probe evidence first.
    async fn list_probes(
        &self,
        tenant_id: &str,
        datasource_id: &str,
        limit: u32,
    ) -> Result<Vec<DatasourceProbe>, BackendError> {
        let _ = (tenant_id, datasource_id, limit);
        Err(BackendError::Configuration(
            "当前存储后端不支持数据源探测记录".into(),
        ))
    }
}
