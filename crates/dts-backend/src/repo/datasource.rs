use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::{CreateDatasourceParams, DatasourceDetail, DatasourceInfo};

/// 引擎所需的原始连接信息（含密码，不外泄到 API）
#[derive(Debug, Clone)]
pub struct RawConnectionInfo {
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database_name: Option<String>,
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
}
