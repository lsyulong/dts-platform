use std::sync::Arc;

use dts_deerflow_client::{ChatMessage, ChatRequest, DeerFlowClient, PlatformContext};
use tokio::sync::mpsc;
use tracing::error;

use crate::config::DeerFlowConfig;
use crate::models::error::AppError;

/// AI 对话服务
#[derive(Clone)]
pub struct AiService {
    client: Option<Arc<DeerFlowClient>>,
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
                                    let _ = tx.send(delta).await;
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
}
