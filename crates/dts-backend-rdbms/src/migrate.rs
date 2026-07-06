use async_trait::async_trait;
use dts_backend::BackendError;

/// 数据库迁移执行器 trait
///
/// 每个 RDBMS 后端实现自己的迁移逻辑（因为 DDL 存在方言差异）。
#[async_trait]
pub trait MigrationRunner: Send + Sync {
    /// 执行数据库迁移
    async fn run_migrations(&self) -> Result<(), BackendError>;
}
