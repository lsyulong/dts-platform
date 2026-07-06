use async_trait::async_trait;

use crate::error::BackendError;

/// 种子数据初始化 trait
///
/// 在数据库为空时插入默认租户、角色、管理员用户和演示数据源。
#[async_trait]
pub trait SeedRepo: Send + Sync {
    /// 检查数据库中是否已有数据
    async fn has_data(&self) -> Result<bool, BackendError>;

    /// 执行种子数据初始化
    async fn seed(&self) -> Result<(), BackendError>;
}
