use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::models::error::AppError;

/// WebSocket 连接配置
#[derive(Clone)]
pub struct WsHub {
    pub tx: broadcast::Sender<(String, String)>,
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    /// 向任务频道推送日志
    pub fn push_log(&self, task_id: &str, level: &str, message: &str) {
        let payload = serde_json::json!({
            "type": "log",
            "task_id": task_id,
            "level": level,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.tx.send((task_id.to_string(), payload.to_string()));
    }

    /// 向任务频道推送进度
    pub fn push_progress(&self, task_id: &str, rows_synced: u64, bytes_synced: u64, total_rows: u64) {
        let percentage = if total_rows > 0 {
            (rows_synced as f64 / total_rows as f64 * 100.0)
        } else {
            0.0
        };

        let payload = serde_json::json!({
            "type": "progress",
            "task_id": task_id,
            "rows_synced": rows_synced,
            "bytes_synced": bytes_synced,
            "total_rows": total_rows,
            "percentage": percentage,
        });
        let _ = self.tx.send((task_id.to_string(), payload.to_string()));
    }
}

/// WebSocket 端点：GET /ws/tasks/{task_id}
pub async fn task_log_ws(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    hub: web::Data<WsHub>,
) -> Result<HttpResponse, AppError> {
    let task_id = path.into_inner();

    log::info!("WebSocket 连接请求: task_id={}", task_id);

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    let task_id_clone = task_id.clone();
    let mut rx = hub.tx.subscribe();

    let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(15));

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok((tid, msg)) if tid == task_id_clone => {
                            if session.text(msg).await.is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("WebSocket 广播落后 {} 条消息", n);
                        }
                        Err(_) => break,
                    }
                }
                _ = heartbeat_interval.tick() => {
                    if session.ping(b"").await.is_err() {
                        break;
                    }
                }
                msg = msg_stream.next() => {
                    match msg {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(Message::Ping(bytes))) => {
                            let _ = session.pong(&bytes).await;
                        }
                        _ => {}
                    }
                }
            }
        }
        log::info!("WebSocket 连接关闭: task_id={}", task_id_clone);
    });

    Ok(response)
}
