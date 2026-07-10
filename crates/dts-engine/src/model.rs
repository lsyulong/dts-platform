use serde::{Deserialize, Serialize};

/// 同步任务提交参数
#[derive(Debug, Clone)]
pub struct TaskSubmission {
    /// 平台任务 ID
    pub task_id: String,
    /// 源数据库连接配置
    pub source: ConnectionConfig,
    /// 目标数据库连接配置
    pub target: ConnectionConfig,
    /// 同步配置（表映射、模式、批量大小）
    pub config: SyncConfig,
}

/// 数据库连接配置
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// 数据库类型：mysql, postgresql 等
    pub db_type: String,
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 明文密码（由 Service 层解密后传入）
    pub password: String,
    /// 数据库名
    pub database: String,
}

/// 同步配置
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// 表映射列表
    pub table_mappings: Vec<TableMapping>,
    /// 同步模式
    pub mode: SyncMode,
    /// 批量大小（行数）
    pub batch_size: usize,
}

/// 表映射
#[derive(Debug, Clone)]
pub struct TableMapping {
    /// 源表名
    pub source_table: String,
    /// 目标表名
    pub target_table: String,
    /// 源 schema（可选，如 PostgreSQL schema）
    pub source_schema: Option<String>,
    /// 目标 schema（可选）
    pub target_schema: Option<String>,
}

/// 同步模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// 全量同步
    Full,
    /// 增量同步
    Incremental,
    /// 全量 + 增量（先全量后持续增量）
    FullThenIncremental,
}

/// 同步进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    /// 运行 ID
    pub run_id: String,
    /// 关联任务 ID
    pub task_id: String,
    /// 当前状态
    pub status: RunStatus,
    /// 已同步行数
    pub rows_synced: u64,
    /// 已同步字节数
    pub bytes_synced: u64,
    /// 预估总行数
    pub total_rows: Option<u64>,
    /// 当前正在处理的表
    pub current_table: Option<String>,
    /// 完成百分比 (0.0 - 100.0)
    pub percent: f64,
    /// 附加消息（如阶段描述、错误信息）
    pub message: Option<String>,
}

/// 运行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}
