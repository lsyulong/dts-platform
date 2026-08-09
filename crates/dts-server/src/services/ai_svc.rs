// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


use std::sync::Arc;

use dts_deerflow_client::{ChatMessage, ChatRequest, DeerFlowClient, PlatformContext, Role};
use tokio::sync::mpsc;
use tracing::error;

use crate::config::DeerFlowConfig;
use crate::models::error::AppError;

/// AI 对话服务
#[derive(Clone)]
pub struct AiService {
    client: Option<Arc<DeerFlowClient>>,
}

/// 任务诊断上下文 — 传递给 DeerFlow 进行根因分析
#[derive(Debug, Clone)]
pub struct DiagnosisContext {
    /// 任务名称
    pub task_name: String,
    /// 任务描述
    pub task_description: Option<String>,
    /// 数据源类型（源端）
    pub source_db_type: String,
    /// 数据源类型（目标端）
    pub target_db_type: String,
    /// 最新运行状态
    pub run_status: String,
    /// 错误消息
    pub error_message: Option<String>,
    /// 已同步行数
    pub rows_synced: i64,
    /// 最近运行历史摘要（status + error_message）
    pub recent_history: Vec<RunHistoryEntry>,
}

#[derive(Debug, Clone)]
pub struct RunHistoryEntry {
    pub status: String,
    pub rows_synced: i64,
    pub error_message: Option<String>,
    pub start_time: String,
}

impl AiService {
    pub fn new(config: &DeerFlowConfig) -> Self {
        let client = if config.enabled && !config.api_key.is_empty() {
            Some(Arc::new(DeerFlowClient::new(
                config.base_url.clone(),
                config.api_key.clone(),
            )))
        } else {
            None
        };
        Self { client }
    }

    /// 流式对话 — 返回 channel receiver
    pub async fn chat_stream(
        &self,
        message: String,
        route: String,
        history: Vec<ChatMessage>,
    ) -> Result<mpsc::Receiver<String>, AppError> {
        let request = ChatRequest {
            message,
            context: Some(PlatformContext {
                current_route: route,
                task_id: None,
                datasource_id: None,
                recent_alerts: 0,
            }),
            history,
        };

        let (tx, rx) = mpsc::channel::<String>(64);
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("AI 服务未启用".into()))?
            .clone();

        tokio::spawn(async move {
            match client.chat_stream(&request).await {
                Ok(mut stream) => {
                    use tokio_stream::StreamExt;
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(chunk) => {
                                if let Some(delta) = chunk.delta {
                                    if tx.send(delta).await.is_err() {
                                        // Receiver disconnected — client likely closed connection
                                        break;
                                    }
                                }
                                if chunk.done {
                                    break;
                                }
                            },
                            Err(e) => {
                                error!(error = %e, "DeerFlow 流错误");
                                let _ = tx.send(format!("\n\n❌ 错误: {e}")).await;
                                break;
                            },
                        }
                    }
                },
                Err(e) => {
                    error!(error = %e, "DeerFlow 请求失败");
                    let _ = tx.send(format!("\n\n❌ AI 服务暂不可用: {e}")).await;
                },
            }
        });

        Ok(rx)
    }

    /// 任务故障诊断 — 流式 SSE 返回根因分析 + 修复建议
    pub async fn diagnose(
        &self,
        ctx: DiagnosisContext,
    ) -> Result<mpsc::Receiver<String>, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("AI 服务未启用".into()))?
            .clone();

        let system_prompt = build_diagnosis_prompt(&ctx);
        let user_message = format!(
            "请分析上述任务 \"{name}\" 的故障原因并给出修复建议。当前状态: {status}。",
            name = ctx.task_name,
            status = ctx.run_status,
        );

        let request = ChatRequest {
            message: user_message,
            context: Some(PlatformContext {
                current_route: "/tasks".into(),
                task_id: None,
                datasource_id: None,
                recent_alerts: 1,
            }),
            history: vec![ChatMessage {
                role: Role::System,
                content: system_prompt,
            }],
        };

        let (tx, rx) = mpsc::channel::<String>(128);

        tokio::spawn(async move {
            match client.chat_stream(&request).await {
                Ok(mut stream) => {
                    use tokio_stream::StreamExt;
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(chunk) => {
                                if let Some(delta) = chunk.delta {
                                    if tx.send(delta).await.is_err() {
                                        break;
                                    }
                                }
                                if chunk.done {
                                    break;
                                }
                            },
                            Err(e) => {
                                error!(error = %e, "DeerFlow diagnosis stream error");
                                let _ = tx
                                    .send(format!("\n\n❌ 诊断服务出错: {e}"))
                                    .await;
                                break;
                            },
                        }
                    }
                },
                Err(e) => {
                    error!(error = %e, "DeerFlow diagnosis request failed");
                    let _ = tx
                        .send(format!("\n\n❌ AI 诊断服务暂不可用: {e}"))
                        .await;
                },
            }
        });

        Ok(rx)
    }
}

/// 构建诊断系统提示词 — 引导 DeerFlow 以数据运维专家身份分析问题
fn build_diagnosis_prompt(ctx: &DiagnosisContext) -> String {
    let mut prompt = String::new();
    prompt.push_str("你是一位资深数据运维专家，负责诊断 DTS Platform 数据同步任务失败的原因。\n\n");
    prompt.push_str("## 任务信息\n\n");
    prompt.push_str(&format!("- 任务名称: {}\n", ctx.task_name));
    if let Some(ref desc) = ctx.task_description {
        prompt.push_str(&format!("- 任务描述: {desc}\n"));
    }
    prompt.push_str(&format!("- 源端数据库: {}\n", ctx.source_db_type));
    prompt.push_str(&format!("- 目标端数据库: {}\n", ctx.target_db_type));
    prompt.push_str(&format!("- 当前状态: {}\n", ctx.run_status));
    prompt.push_str(&format!("- 已同步行数: {}\n", ctx.rows_synced));

    if let Some(ref err) = ctx.error_message {
        prompt.push_str(&format!("\n## 最新错误信息\n\n```\n{err}\n```\n"));
    }

    if !ctx.recent_history.is_empty() {
        prompt.push_str("\n## 最近运行历史\n\n");
        prompt.push_str("| 开始时间 | 状态 | 同步行数 | 错误 |\n");
        prompt.push_str("|----------|------|----------|------|\n");
        for entry in &ctx.recent_history {
            let err = entry.error_message.as_deref().unwrap_or("-");
            prompt.push_str(&format!(
                "| {} | {} | {} | {err} |\n",
                entry.start_time, entry.status, entry.rows_synced,
            ));
        }
    }

    prompt.push_str(
        "\n## 要求\n\n\
        请按以下格式输出诊断结果：\n\n\
        1. **根因分析**: 基于错误信息和运行历史，判断最可能的失败原因\n\
        2. **影响评估**: 对数据一致性和下游依赖的影响\n\
        3. **修复建议**: 具体的操作步骤（按优先级排列）\n\
        4. **预防措施**: 如何避免类似问题再次发生\n\n\
        用中文回答，保持专业但易懂。",
    );

    prompt
}
