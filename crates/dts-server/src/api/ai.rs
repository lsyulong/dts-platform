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
