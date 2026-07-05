use actix_web::{HttpRequest, HttpResponse, web};
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
    pub fn push_progress(
        &self,
        task_id: &str,
        rows_synced: u64,
        bytes_synced: u64,
        total_rows: u64,
    ) {
        let percentage = if total_rows > 0 {
            rows_synced as f64 / total_rows as f64 * 100.0
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== WsHub::new() Tests ==========

    #[test]
    fn ws_hub_new_creates_valid_hub() {
        let hub = WsHub::new();
        // Verify we can get a receiver
        let rx = hub.tx.subscribe();
        assert_eq!(rx.len(), 0);
    }

    #[test]
    fn ws_hub_new_senders_can_be_cloned() {
        let hub = WsHub::new();
        let _tx2 = hub.tx.clone();
        // The hub should have at least 1 sender (from new) + 1 cloned
        // We just verify cloning doesn't panic
    }

    // ========== push_log Tests ==========

    #[test]
    fn push_log_sends_correctly_formatted_json() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_log("task-1", "info", "test message");

        let received = rx.try_recv().unwrap();
        assert_eq!(received.0, "task-1");

        let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
        assert_eq!(payload["type"], "log");
        assert_eq!(payload["task_id"], "task-1");
        assert_eq!(payload["level"], "info");
        assert_eq!(payload["message"], "test message");
        // Check timestamp exists and is a string
        assert!(payload["timestamp"].is_string());
    }

    #[test]
    fn push_log_can_send_multiple_times() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_log("task-1", "info", "first");
        hub.push_log("task-1", "warn", "second");
        hub.push_log("task-2", "error", "third");

        let msgs: Vec<_> = (0..3).map(|_| rx.try_recv().unwrap()).collect();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].0, "task-1");
        assert_eq!(msgs[1].0, "task-1");
        assert_eq!(msgs[2].0, "task-2");
    }

    #[test]
    fn push_log_with_different_levels_preserves_level() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_log("t1", "debug", "debug msg");
        hub.push_log("t1", "info", "info msg");
        hub.push_log("t1", "error", "error msg");

        let levels: Vec<String> = (0..3)
            .map(|_| {
                let msg = rx.try_recv().unwrap();
                let payload: serde_json::Value = serde_json::from_str(&msg.1).unwrap();
                payload["level"].as_str().unwrap().to_string()
            })
            .collect();
        assert_eq!(levels, vec!["debug", "info", "error"]);
    }

    #[test]
    fn push_log_with_empty_message() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_log("task-1", "info", "");

        let received = rx.try_recv().unwrap();
        let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
        assert_eq!(payload["message"], "");
        assert_eq!(payload["type"], "log");
    }

    // ========== push_progress Tests ==========

    #[test]
    fn push_progress_sends_correctly_formatted_json() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_progress("task-1", 50, 1024, 100);

        let received = rx.try_recv().unwrap();
        assert_eq!(received.0, "task-1");

        let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
        assert_eq!(payload["type"], "progress");
        assert_eq!(payload["task_id"], "task-1");
        assert_eq!(payload["rows_synced"], 50);
        assert_eq!(payload["bytes_synced"], 1024);
        assert_eq!(payload["total_rows"], 100);
        assert!((payload["percentage"].as_f64().unwrap() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn push_progress_calculates_percentage_correctly() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        let cases = vec![
            (0, 100, 0.0),
            (25, 100, 25.0),
            (50, 100, 50.0),
            (75, 100, 75.0),
            (100, 100, 100.0),
            (33, 100, 33.0),
        ];

        for (rows_synced, total_rows, expected_pct) in cases {
            hub.push_progress("t1", rows_synced, 0, total_rows);
            let received = rx.try_recv().unwrap();
            let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
            let pct = payload["percentage"].as_f64().unwrap();
            assert!(
                (pct - expected_pct).abs() < 0.01,
                "Expected {}%, got {}% for {}/{}",
                expected_pct,
                pct,
                rows_synced,
                total_rows
            );
        }
    }

    #[test]
    fn push_progress_total_rows_zero_returns_zero_percent() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_progress("task-1", 10, 256, 0);

        let received = rx.try_recv().unwrap();
        let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
        assert_eq!(payload["percentage"].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn push_progress_with_large_numbers() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_progress("task-1", 500_000_000, 10_000_000_000, 1_000_000_000);

        let received = rx.try_recv().unwrap();
        let payload: serde_json::Value = serde_json::from_str(&received.1).unwrap();
        let pct = payload["percentage"].as_f64().unwrap();
        assert!((pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn push_log_does_not_error_when_no_receiver() {
        let hub = WsHub::new();
        // No active receiver — push should not panic
        hub.push_log("task-1", "info", "message");
        hub.push_progress("task-1", 10, 100, 100);
    }

    // ========== push_log to different tasks Tests ==========

    #[test]
    fn push_log_to_different_tasks_routes_correctly() {
        let hub = WsHub::new();
        let mut rx = hub.tx.subscribe();

        hub.push_log("task-A", "info", "message A");
        hub.push_log("task-B", "error", "message B");

        let first = rx.try_recv().unwrap();
        let second = rx.try_recv().unwrap();

        assert_eq!(first.0, "task-A");
        assert_eq!(second.0, "task-B");
    }
}
