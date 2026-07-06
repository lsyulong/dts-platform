use async_trait::async_trait;

use crate::error::BackendError;
use crate::types::{CreateTenantParams, TenantInfo};

/// 租户数据访问 trait
///
/// 所有存储后端实现必须提供此 trait 的具体实现。
#[async_trait]
pub trait TenantRepo: Send + Sync {
    /// 创建租户，返回租户 ID
    async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError>;

    /// 根据 slug 查找租户
    async fn find_by_slug(&self, slug: &str) -> Result<Option<TenantInfo>, BackendError>;

    /// 根据 ID 查找租户
    async fn find_by_id(&self, id: &str) -> Result<Option<TenantInfo>, BackendError>;

    /// 获取所有租户（按创建时间倒序）
    async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError>;

    /// 租户总数
    async fn count(&self) -> Result<i64, BackendError>;
}
