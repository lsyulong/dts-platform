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
use actix_ws::Message;
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;
use crate::services::task_log_svc::{RunLogScope, TaskLogService};
use dts_backend::TaskRepo;
use std::sync::Arc;

/// WebSocket 连接配置
#[derive(Clone)]
pub struct WsHub {
    pub tx: broadcast::Sender<(String, String)>,
    log_service: Option<TaskLogService>,
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            tx,
            log_service: None,
        }
    }

    pub fn with_log_service(service: TaskLogService) -> Self {
        Self {
            log_service: Some(service),
            ..Self::new()
        }
    }

    pub async fn push_run_log(
        &self,
        scope: RunLogScope<'_>,
        level: &str,
        component: &str,
        message: &str,
    ) -> Result<(), dts_backend::BackendError> {
        let service = self.log_service.as_ref().ok_or_else(|| {
            dts_backend::BackendError::Configuration("未配置运行日志持久化".into())
        })?;
        let log = service.record(scope, level, component, message).await?;
        let payload = serde_json::json!({ "type": "log", "task_id": scope.task_id, "run_id": scope.run_id, "id": log.id, "sequence": log.sequence, "timestamp": log.timestamp, "level": log.level, "component": log.component, "message": log.message });
        let _ = self.tx.send((scope.task_id.into(), payload.to_string()));
        Ok(())
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
    let bearer = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            req.headers()
                .get("Sec-WebSocket-Protocol")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    value
                        .split(',')
                        .find_map(|part| part.trim().strip_prefix("bearer."))
                })
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Unauthorized("任务日志连接需要认证".into()))?;
    let auth = req
        .app_data::<web::Data<AuthService>>()
        .ok_or_else(|| AppError::Internal("认证服务未配置".into()))?;
    let claims = auth.authenticate(bearer).await?;
    auth.authorize(&claims, "task:read").await?;
    let task_id = path.into_inner();
    let repo = req
        .app_data::<web::Data<Arc<dyn TaskRepo>>>()
        .ok_or_else(|| AppError::Internal("任务存储未配置".into()))?;
    repo.find_detail_by_id(&task_id, &claims.tenant_id)
        .await?
        .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
    let bearer = bearer.to_owned();
    let auth = auth.get_ref().clone();
    let repo = repo.get_ref().clone();

    tracing::info!("WebSocket 连接请求: task_id={task_id}");

    let (mut response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    if req
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|part| part.trim() == "dts.logs"))
    {
        response.headers_mut().insert(
            actix_web::http::header::SEC_WEBSOCKET_PROTOCOL,
            actix_web::http::header::HeaderValue::from_static("dts.logs"),
        );
    }
    let mut rx = hub.tx.subscribe();

    let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(15));

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok((tid, msg)) if tid == task_id => {
                            if session.text(msg).await.is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("WebSocket 广播落后 {n} 条消息");
                        }
                        Err(_) => break,
                    }
                }
                _ = heartbeat_interval.tick() => {
                    let authorized = async {
                        let identity = auth.authenticate(&bearer).await?;
                        auth.authorize(&identity, "task:read").await?;
                        repo.find_detail_by_id(&task_id, &identity.tenant_id).await?
                            .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
                        Ok::<_, AppError>(())
                    }.await;
                    if authorized.is_err() {
                        let _ = session.close(Some(actix_ws::CloseReason {
                            code: actix_ws::CloseCode::Policy,
                            description: Some("任务日志访问权限已失效".into()),
                        })).await;
                        break;
                    }
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
        tracing::info!("WebSocket 连接关闭: task_id={task_id}");
    });

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    struct FailingLogRepo;

    #[async_trait::async_trait]
    impl dts_backend::repo::TaskLogRepo for FailingLogRepo {
        async fn append(
            &self,
            _: &dts_backend::repo::task_log::AppendTaskLog,
        ) -> Result<dts_backend::repo::task_log::TaskLogRecord, dts_backend::BackendError> {
            Err(dts_backend::BackendError::Internal(
                "test storage failure".into(),
            ))
        }
        async fn list(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: i64,
            _: i64,
        ) -> Result<dts_backend::repo::task_log::TaskLogPage, dts_backend::BackendError> {
            Err(dts_backend::BackendError::Internal(
                "test storage failure".into(),
            ))
        }
    }

    #[tokio::test]
    async fn persistence_failure_and_missing_service_never_broadcast_success_logs() {
        let scope = RunLogScope {
            tenant_id: "tenant",
            task_id: "task",
            run_id: "run",
        };
        for hub in [
            WsHub::new(),
            WsHub::with_log_service(TaskLogService::new(Arc::new(FailingLogRepo))),
        ] {
            let mut receiver = hub.tx.subscribe();
            assert!(
                hub.push_run_log(scope, "INFO", "scheduler", "observed event")
                    .await
                    .is_err()
            );
            assert!(matches!(
                receiver.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        }
    }

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
        let _tx2 = hub.tx;
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
                "Expected {expected_pct}%, got {pct}% for {rows_synced}/{total_rows}"
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
