use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::MonitoringOverview;

/// 监控数据访问 trait
#[async_trait]
pub trait MonitoringRepo: Send + Sync {
    /// 获取租户监控概览
    async fn get_overview(&self, tenant_id: &str) -> Result<MonitoringOverview, BackendError>;
}
