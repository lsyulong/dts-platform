use crate::error::BackendError;

/// 存储后端连接池抽象
///
/// 各协议实现（sqlite, postgres, mysql, oracle 等）需要提供对应的连接池类型，
/// 并通过 `ping` 方法执行健康检查。
///
/// 上层代码通过 `Arc<dyn BackendPool>` 持有，不依赖具体后端类型。
#[async_trait::async_trait]
pub trait BackendPool: Send + Sync + 'static {
    /// 后端协议名称，如 "postgres"、"mysql"、"oracle"
    fn backend_name(&self) -> &'static str;

    /// 健康检查 — ping 数据库
    async fn ping(&self) -> Result<(), BackendError>;
}

/// 可创建连接池的工厂 trait
#[async_trait::async_trait]
pub trait BackendPoolFactory: Send + Sync {
    type Pool: BackendPool;

    /// 从数据库 URL 创建连接池
    async fn create_pool(database_url: &str) -> Result<Self::Pool, BackendError>;
}
