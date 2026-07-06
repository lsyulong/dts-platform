/// 存储后端统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("连接失败: {0}")]
    Connection(String),

    #[error("查询失败: {0}")]
    Query(String),

    #[error("记录不存在: {0}")]
    NotFound(String),

    #[error("记录已存在: {0}")]
    AlreadyExists(String),

    #[error("约束冲突: {0}")]
    ConstraintViolation(String),

    #[error("迁移失败: {0}")]
    Migration(String),

    #[error("配置错误: {0}")]
    Configuration(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl BackendError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::AlreadyExists(_))
    }
}
