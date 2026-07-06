use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::{AlertInfo, CreateAlertParams};

/// 告警规则数据访问 trait
#[async_trait]
pub trait AlertRepo: Send + Sync {
    /// 创建告警规则
    async fn create(&self, params: &CreateAlertParams) -> Result<String, BackendError>;

    /// 获取租户告警规则列表
    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<AlertInfo>, BackendError>;

    /// 切换告警规则启用状态
    async fn toggle(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError>;
}
