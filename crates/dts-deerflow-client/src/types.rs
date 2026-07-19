use serde::{Deserialize, Serialize};

/// 对话消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// DeerFlow 对话请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// 用户消息
    pub message: String,
    /// DTS 平台上下文（页面、任务状态等）
    pub context: Option<PlatformContext>,
    /// 对话历史
    pub history: Vec<ChatMessage>,
}

/// DTS 平台上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformContext {
    /// 当前页面路由
    pub current_route: String,
    /// 任务 ID（如果在任务详情页）
    pub task_id: Option<String>,
    /// 数据源 ID（如果在数据源页）
    pub datasource_id: Option<String>,
    /// 最近告警数量
    pub recent_alerts: usize,
}

/// 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// SSE 流式响应块
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamChunk {
    /// 增量文本内容
    pub delta: Option<String>,
    /// 是否最终块
    pub done: bool,
    /// 完整响应（done=true 时提供）
    pub content: Option<String>,
}
