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

use std::time::Duration;

use anyhow::{Context, Result};
use futures_core::Stream;
use reqwest::Client as HttpClient;
use tokio_stream::StreamExt;
use tracing::debug;

use crate::types::{ChatRequest, StreamChunk};

/// HTTP 请求默认连接超时（秒）
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;
/// HTTP 请求默认总超时（秒）
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 120;

/// DeerFlow HTTP 客户端
#[derive(Clone)]
pub struct DeerFlowClient {
    base_url: String,
    api_key: String,
    http_client: HttpClient,
}

impl DeerFlowClient {
    /// 创建客户端实例
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let http_client = match HttpClient::builder()
            .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .build()
        {
            Ok(client) => client,
            Err(e) => panic!("failed to build reqwest client: {e}"),
        };
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            http_client,
        }
    }

    /// 发送对话请求，返回流式响应
    pub async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<impl Stream<Item = Result<StreamChunk>>> {
        let url = format!("{}/api/v1/chat/completions", self.base_url);
        debug!(url = %url, "sending DeerFlow chat request");

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .with_context(|| format!("DeerFlow request failed: {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp
                .text()
                .await
                .with_context(|| format!("failed to read DeerFlow error body (HTTP {status})"))?;
            anyhow::bail!("DeerFlow error (HTTP {status}): {body}");
        }

        // 按 SSE 行解析字节流
        let byte_stream = resp.bytes_stream();
        let stream = byte_stream.map(|chunk| {
            let bytes = chunk.context("DeerFlow stream read failed")?;
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|e| anyhow::anyhow!("DeerFlow SSE chunk is not valid UTF-8: {e}"))?;
            parse_sse_line(&text).ok_or_else(|| anyhow::anyhow!("SSE parse failed: {text}"))
        });

        Ok(stream)
    }

    /// 发送同步对话请求（非流式）
    pub async fn chat(&self, request: &ChatRequest) -> Result<String> {
        let url = format!("{}/api/v1/chat", self.base_url);
        debug!(url = %url, "sending DeerFlow sync request");

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(request)
            .send()
            .await
            .with_context(|| format!("DeerFlow request failed: {url}"))?;

        let body = resp.text().await?;
        Ok(body)
    }
}

/// 解析 SSE `data:` 行，提取 JSON StreamChunk。
///
/// # 局限性（Phase 3a）
///
/// - 每个缓冲区仅处理第一条 `data:` 行；多行 SSE 事件暂不支持。
/// - SSE 注释行 (`: ...`)、`event:`、`id:`、`retry:` 字段
///   会被忽略并导致返回 `None`。
/// - 假设每个 TCP 数据块最多包含一个 SSE 事件。
///
/// Phase 3b 可考虑迁移至完整的 SSE 解析库（例如 `eventsource-stream`）。
#[doc(hidden)]
#[must_use]
pub fn parse_sse_line(text: &str) -> Option<StreamChunk> {
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                return Some(StreamChunk {
                    delta: None,
                    done: true,
                    content: None,
                });
            }
            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                return Some(chunk);
            }
        }
    }
    None
}
