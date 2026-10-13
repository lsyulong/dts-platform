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

/// 数据分析报告上下文
#[derive(Debug, Clone)]
pub struct ReportContext {
    pub task_name: String,
    pub task_description: Option<String>,
    pub source_db_type: String,
    pub target_db_type: String,
    pub total_runs: usize,
    pub success_rate: f64,
    pub total_rows_synced: i64,
    pub avg_rows_per_run: i64,
    pub recent_runs: Vec<ReportRunSummary>,
    pub period_hours: i64,
}

#[derive(Debug, Clone)]
pub struct ReportRunSummary {
    pub status: String,
    pub rows_synced: i64,
    pub duration_secs: Option<i64>,
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

    /// 生成数据分析报告 — 流式 SSE 返回 Markdown 格式报告
    pub async fn generate_report(
        &self,
        ctx: ReportContext,
    ) -> Result<mpsc::Receiver<String>, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("AI 服务未启用".into()))?
            .clone();

        let system_prompt = build_report_prompt(&ctx);
        let user_message = format!(
            "请为任务 \"{name}\" 生成一份数据同步分析报告。",
            name = ctx.task_name,
        );

        let request = ChatRequest {
            message: user_message,
            context: Some(PlatformContext {
                current_route: "/tasks".into(),
                task_id: None,
                datasource_id: None,
                recent_alerts: 0,
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
                                error!(error = %e, "DeerFlow report stream error");
                                let _ = tx.send(format!("\n\n❌ 报告生成出错: {e}")).await;
                                break;
                            },
                        }
                    }
                },
                Err(e) => {
                    error!(error = %e, "DeerFlow report request failed");
                    let _ = tx.send(format!("\n\n❌ AI 报告服务暂不可用: {e}")).await;
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
                                let _ = tx.send(format!("\n\n❌ 诊断服务出错: {e}")).await;
                                break;
                            },
                        }
                    }
                },
                Err(e) => {
                    error!(error = %e, "DeerFlow diagnosis request failed");
                    let _ = tx.send(format!("\n\n❌ AI 诊断服务暂不可用: {e}")).await;
                },
            }
        });

        Ok(rx)
    }
}

/// 构建报告生成系统提示词
fn build_report_prompt(ctx: &ReportContext) -> String {
    let mut prompt = String::new();
    prompt.push_str("你是一位资深数据工程师，负责生成 DTS Platform 数据同步分析报告。\n\n");
    prompt.push_str("## 任务信息\n\n");
    prompt.push_str(&format!("- 任务名称: {}\n", ctx.task_name));
    if let Some(ref desc) = ctx.task_description {
        prompt.push_str(&format!("- 任务描述: {desc}\n"));
    }
    prompt.push_str(&format!("- 源端数据库: {}\n", ctx.source_db_type));
    prompt.push_str(&format!("- 目标端数据库: {}\n", ctx.target_db_type));
    prompt.push_str(&format!("- 统计周期: 最近 {} 小时\n", ctx.period_hours));
    prompt.push_str(&format!("- 总运行次数: {}\n", ctx.total_runs));
    prompt.push_str(&format!("- 成功率: {:.1}%\n", ctx.success_rate * 100.0));
    prompt.push_str(&format!("- 累计同步行数: {}\n", ctx.total_rows_synced));
    prompt.push_str(&format!("- 平均每轮同步: {} 行\n", ctx.avg_rows_per_run));

    if !ctx.recent_runs.is_empty() {
        prompt.push_str("\n## 最近运行记录\n\n");
        prompt.push_str("| 开始时间 | 状态 | 同步行数 | 耗时 |\n");
        prompt.push_str("|----------|------|----------|------|\n");
        for run in &ctx.recent_runs {
            let duration = run
                .duration_secs
                .map(|s| format!("{s}s"))
                .unwrap_or_else(|| "-".to_string());
            prompt.push_str(&format!(
                "| {} | {} | {} | {duration} |\n",
                run.start_time, run.status, run.rows_synced,
            ));
        }
    }

    prompt.push_str(
        "\n## 要求\n\n\
        请生成一份专业的 Markdown 格式数据分析报告，包含以下章节：\n\n\
        ## 📊 执行摘要\n\
        一段话概括任务运行状况（100字以内）\n\n\
        ## 📈 趋势分析\n\
        分析最近运行的数据量变化趋势，是否有增长或下降\n\n\
        ## ⚠️ 异常检测\n\
        识别异常模式：失败率上升、数据量突增/骤降、执行时间变长\n\n\
        ## 💡 优化建议\n\
        基于分析结果，给出2-3条可操作的优化建议\n\n\
        ## 📋 数据概览\n\
        用表格汇总关键指标\n\n\
        用中文回答，专业但易懂。每章节用 ## 标题。",
    );

    prompt
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_basic_context() -> DiagnosisContext {
        DiagnosisContext {
            task_name: "test-sync".into(),
            task_description: Some("MySQL → PostgreSQL 全量同步".into()),
            source_db_type: "mysql".into(),
            target_db_type: "postgresql".into(),
            run_status: "failed".into(),
            error_message: Some("Connection refused (os error 111)".into()),
            rows_synced: 1000,
            recent_history: Vec::new(),
        }
    }

    #[test]
    fn prompt_contains_task_info() {
        let ctx = make_basic_context();
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(prompt.contains("test-sync"));
        assert!(prompt.contains("MySQL → PostgreSQL 全量同步"));
        assert!(prompt.contains("mysql"));
        assert!(prompt.contains("postgresql"));
        assert!(prompt.contains("failed"));
        assert!(prompt.contains("1000"));
    }

    #[test]
    fn prompt_contains_error_message() {
        let ctx = make_basic_context();
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(prompt.contains("Connection refused"));
        assert!(prompt.contains("最新错误信息"));
    }

    #[test]
    fn prompt_without_error_skips_error_section() {
        let mut ctx = make_basic_context();
        ctx.error_message = None;
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(!prompt.contains("最新错误信息"));
    }

    #[test]
    fn prompt_without_description_skips_description() {
        let mut ctx = make_basic_context();
        ctx.task_description = None;
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(!prompt.contains("任务描述"));
    }

    #[test]
    fn prompt_contains_history_table() {
        let mut ctx = make_basic_context();
        ctx.recent_history = vec![
            RunHistoryEntry {
                status: "completed".into(),
                rows_synced: 5000,
                error_message: None,
                start_time: "2026-08-09T10:00:00Z".into(),
            },
            RunHistoryEntry {
                status: "failed".into(),
                rows_synced: 1000,
                error_message: Some("timeout".into()),
                start_time: "2026-08-09T11:00:00Z".into(),
            },
        ];

        let prompt = build_diagnosis_prompt(&ctx);

        assert!(prompt.contains("最近运行历史"));
        assert!(prompt.contains("2026-08-09T10:00:00Z"));
        assert!(prompt.contains("completed"));
        assert!(prompt.contains("5000"));
        assert!(prompt.contains("2026-08-09T11:00:00Z"));
        assert!(prompt.contains("timeout"));
    }

    #[test]
    fn prompt_empty_history_skips_table() {
        let ctx = make_basic_context();
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(!prompt.contains("最近运行历史"));
    }

    #[test]
    fn prompt_ends_with_requirements() {
        let ctx = make_basic_context();
        let prompt = build_diagnosis_prompt(&ctx);

        assert!(prompt.contains("根因分析"));
        assert!(prompt.contains("影响评估"));
        assert!(prompt.contains("修复建议"));
        assert!(prompt.contains("预防措施"));
    }
}
