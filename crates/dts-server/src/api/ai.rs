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


use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::middleware::auth;
use crate::models::error::AppError;

#[derive(Deserialize)]
pub struct AiChatQuery {
    pub message: String,
    #[serde(default)]
    pub route: Option<String>,
}

/// POST /api/v1/ai/chat — SSE 流式 AI 对话
pub async fn chat(
    ai_svc: web::Data<crate::services::ai_svc::AiService>,
    req: HttpRequest,
    query: web::Query<AiChatQuery>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = auth::get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;

    let route = query.route.clone().unwrap_or_default();
    let rx = ai_svc
        .chat_stream(query.message.clone(), route, vec![])
        .await?;

    // 使用 tokio_stream ReceiverStream 适配 actix SSE
    let receiver_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let sse_stream = receiver_stream.map(|text| {
        let data = format!("data: {}\n\n", text.replace('\n', "\\n"));
        Ok::<_, actix_web::Error>(web::Bytes::from(data))
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(sse_stream))
}
