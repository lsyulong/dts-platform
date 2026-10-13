// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use dts_scheduler_client::{JiaSchedulerClient, TaskSync};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Default)]
struct State {
    requests: Vec<(String, Value)>,
    job: Option<Value>,
    timer: Option<Value>,
    runtime: Option<Value>,
    fail_login: bool,
    fail_delete: bool,
    fail_start: bool,
    fail_stop: bool,
}

#[tokio::test]
async fn invalid_periodic_definitions_never_save_or_dispatch_remote_work() {
    use dts_scheduler_client::models::{SaveTimerReq, TimerExpr};
    let mock = MockCommunity::start(false, false).await;
    let client = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .expect("login");
    let sync = TaskSync::new(client.clone(), "worker-1")
        .await
        .expect("executor");
    let before = mock.state.lock().expect("state").requests.len();
    for expression in [
        "0 25 * * *",
        "*/0 * * * *",
        "0 0 0 * * UNKNOWN",
        "0 0 0 * * * 2026",
    ] {
        assert!(
            sync.upsert_job(
                "task-1",
                "fixture",
                "http://platform/start",
                "fixture",
                Some(expression)
            )
            .await
            .is_err()
        );
    }
    let mut timer = TimerExpr::from_cron("*/5 * * * *").expect("timer");
    timer.timezone = "Asia/Shanghai".into();
    assert!(
        client
            .save_timer(&SaveTimerReq {
                name: "DTS-Timer-task-1".into(),
                eid: "community-generated-42".into(),
                job_type: "default".into(),
                timer_expr: timer,
                job_args: vec![],
                info: String::new(),
                id: None,
            })
            .await
            .is_err()
    );
    let state = mock.state.lock().expect("state");
    assert_eq!(state.requests.len(), before);
    assert!(state.job.is_none());
    assert!(state.timer.is_none());
    assert!(state.runtime.is_none());
}

struct MockCommunity {
    url: String,
    state: Arc<Mutex<State>>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for MockCommunity {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl MockCommunity {
    async fn start(fail_login: bool, fail_delete: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
        let url = format!("http://{}", listener.local_addr().expect("mock address"));
        let state = Arc::new(Mutex::new(State {
            fail_login,
            fail_delete,
            ..State::default()
        }));
        let shared = Arc::clone(&state);
        let server = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let state = Arc::clone(&shared);
                tokio::spawn(async move {
                    serve(socket, state).await;
                });
            }
        });
        Self { url, state, server }
    }
}

async fn read_request(socket: &mut TcpStream) -> (String, String, Value) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let header_end = loop {
        let n = socket.read(&mut buffer).await.expect("read mock request");
        assert_ne!(n, 0, "request closed before headers");
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let length = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("length"))
        })
        .unwrap_or(0);
    while bytes.len() < header_end + length {
        let n = socket.read(&mut buffer).await.expect("read mock body");
        assert_ne!(n, 0, "request closed before body");
        bytes.extend_from_slice(&buffer[..n]);
    }
    let path = headers
        .lines()
        .next()
        .expect("request line")
        .split_whitespace()
        .nth(1)
        .expect("path")
        .to_owned();
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + length]).expect("JSON body")
    };
    (path, headers, body)
}

async fn serve(mut socket: TcpStream, state: Arc<Mutex<State>>) {
    let (path, headers, body) = read_request(&mut socket).await;
    let response = {
        let mut state = state.lock().expect("state");
        state.requests.push((path.clone(), body.clone()));
        if path != "/api/user/login" {
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("cookie: mock_session=ok"),
                "session cookie required"
            );
        }
        community_response(&mut state, path.split('?').next().expect("path"), body)
    };
    let text = response.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: mock_session=ok; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    socket
        .write_all(response.as_bytes())
        .await
        .expect("write response");
}

fn ok(data: Value) -> Value {
    json!({ "code": 20000, "data": data, "msg": "success" })
}
fn error() -> Value {
    json!({ "code": 50001, "data": null, "msg": "sensitive mock error must not escape" })
}

fn community_response(state: &mut State, path: &str, body: Value) -> Value {
    match path {
        "/api/user/login" => {
            if state.fail_login {
                error()
            } else {
                ok(json!({}))
            }
        },
        "/api/executor/list" => ok(
            json!({ "total": 1, "list": [{"id": 9, "name": "dts-shell", "command": "bash -c", "platform": "linux"}] }),
        ),
        "/api/instance/list" => {
            ok(json!({"total": 1, "list": [{"instance_id": "worker-1", "status": 1}]}))
        },
        "/api/job/schedule-list" => ok(
            json!({"total": usize::from(state.runtime.is_some()), "list": state.runtime.iter().collect::<Vec<_>>()}),
        ),
        "/api/job/dispatch" => {
            assert_eq!(body["eid"], "community-generated-42");
            assert_eq!(body["action"], "start_timer");
            assert_eq!(body["endpoints"], json!([{"instance_id": "worker-1"}]));
            assert_eq!(body["timer_expr"]["timezone"], "utc");
            assert!(state.timer.is_some(), "save template before activation");
            if state.fail_start {
                return error();
            }
            assert!(
                state.runtime.is_none(),
                "existing runtime must be updated instead of duplicated"
            );
            state.runtime = Some(
                json!({"id": 88, "eid": body["eid"], "name": body["schedule_name"], "job_type": "default", "schedule_type": "timer", "action": "start_timer", "instance_ids": ["worker-1"], "timer_expr": {"timezone": "utc", "expr": "0 */5 * * * *"}}),
            );
            ok(json!({"result": 100}))
        },
        "/api/job/schedule" => {
            assert_eq!(
                body["schedule_pid"], 88,
                "must use runtime PID, not dispatch history ID"
            );
            if body["action"] == "stop_timer" && state.fail_stop {
                return error();
            }
            if body["action"] == "start_timer" && state.fail_start {
                return error();
            }
            state.runtime.as_mut().expect("runtime")["action"] = body["action"].clone();
            ok(json!({"result": 101}))
        },
        "/api/job/save-schedule" => {
            assert_eq!(body["id"], 88);
            assert_eq!(
                state.runtime.as_ref().expect("runtime")["action"],
                "stop_timer",
                "stop old worker before changing its snapshot"
            );
            assert_eq!(body["endpoints"], json!([{"instance_id": "worker-1"}]));
            assert_eq!(
                body["timer_expr"],
                json!({"timezone": "utc", "expr": "0 */5 * * * *"})
            );
            state.runtime.as_mut().expect("runtime")["timer_expr"] = body["timer_expr"].clone();
            ok(json!({"result": 88, "next_exec_times": []}))
        },
        "/api/job/running-status-list" => ok(json!({"total": 2, "list": [
            {"eid": "community-generated-42", "instance_id": "worker-1", "is_online": true, "schedule_name": "DTS-Timer-task-1", "schedule_status": "scheduling", "next_time": "2026/10/08 14:00:00"},
            {"eid": "foreign-job", "instance_id": "worker-1", "is_online": true, "schedule_name": "DTS-Timer-task-1", "schedule_status": "scheduling", "next_time": "2026/10/08 14:00:00"}
        ]})),
        "/api/job/list" => ok(
            json!({ "total": usize::from(state.job.is_some()), "list": state.job.iter().collect::<Vec<_>>() }),
        ),
        "/api/job/save" => {
            assert!(body["args"].is_array(), "upstream requires args array");
            assert_eq!(body["max_retry"], 0, "sync is not idempotent");
            if state.job.is_some() {
                assert_eq!(body["id"], 42, "must update existing job");
            }
            state.job =
                Some(json!({"id": 42, "eid": "community-generated-42", "name": body["name"]}));
            ok(json!({"result": 42}))
        },
        "/api/job/timer-list" => ok(
            json!({"total": usize::from(state.timer.is_some()), "list": state.timer.iter().collect::<Vec<_>>() }),
        ),
        "/api/job/save-timer" => {
            assert_eq!(body["eid"], "community-generated-42");
            assert_eq!(body["timer_expr"]["timezone"], "utc");
            assert_eq!(body["timer_expr"]["second"], "0");
            assert_eq!(body["timer_expr"]["minute"], "*/5");
            assert_eq!(body["timer_expr"]["year"], "*");
            assert!(body["job_args"].is_array());
            if state.timer.is_some() {
                assert_eq!(body["id"], 73, "must update existing timer");
            }
            state.timer = Some(
                json!({"id": 73, "eid": body["eid"], "name": body["name"], "job_name": "job", "job_type": "default", "timer_expr": body["timer_expr"], "info": "", "created_user": "admin", "created_time": "2026-10-08", "updated_time": "2026-10-08"}),
            );
            ok(json!({"result": 73, "next_exec_times": []}))
        },
        "/api/job/delete-timer" => {
            if state.fail_delete {
                return error();
            }
            assert_eq!(body["id"], 73);
            assert_eq!(
                state.runtime.as_ref().expect("runtime")["action"],
                "stop_timer",
                "deleting template alone does not stop scheduler"
            );
            state.timer = None;
            ok(json!({"result": 1}))
        },
        "/api/job/schedule-history-list" => ok(json!({"total": 2, "list": [
            {"id": 1, "eid": "community-generated-42", "name": "DTS-Timer-task-1", "schedule_type": "timer", "created_time": "2026-10-08"},
            {"id": 2, "eid": "foreign-job", "name": "DTS-Timer-task-1", "schedule_type": "timer", "created_time": "2026-10-08"}
        ]})),
        _ => error(),
    }
}

#[tokio::test]
async fn upsert_reuses_real_community_ids_and_typed_timer_protocol() {
    let mock = MockCommunity::start(false, false).await;
    let client = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .expect("login");
    let sync = TaskSync::new(client.clone(), "worker-1")
        .await
        .expect("executor");
    for _ in 0..2 {
        let eid = sync
            .upsert_job(
                "task-1",
                "fixture",
                "http://platform/internal/start",
                "scoped-fixture-grant",
                Some("*/5 * * * *"),
            )
            .await
            .expect("upsert");
        assert_eq!(eid, "community-generated-42");
    }
    let history = client
        .query_schedule_history("community-generated-42", "DTS-Timer-task-1")
        .await
        .expect("history");
    assert_eq!(history.list.len(), 1, "another job history must not leak");
    let runs = client
        .running_status("community-generated-42", "DTS-Timer-task-1")
        .await
        .expect("status");
    assert_eq!(
        runs.len(),
        1,
        "foreign runtime must not make this task appear active"
    );
    sync.remove_timer("task-1").await.expect("delete");
    assert!(mock.state.lock().expect("state").timer.is_none());
}

#[tokio::test]
async fn business_error_in_http_200_is_not_accepted_as_login_success() {
    let mock = MockCommunity::start(true, false).await;
    let error = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .err()
        .expect("must fail");
    assert!(error.to_string().contains("50001"));
    assert!(!error.to_string().contains("sensitive mock error"));
}

#[tokio::test]
async fn failed_timer_delete_is_returned_to_the_platform() {
    let mock = MockCommunity::start(false, true).await;
    let client = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .expect("login");
    let sync = TaskSync::new(client, "worker-1").await.expect("executor");
    sync.upsert_job(
        "task-1",
        "fixture",
        "http://platform/start",
        "fixture",
        Some("*/5 * * * *"),
    )
    .await
    .expect("upsert");
    assert!(sync.remove_timer("task-1").await.is_err());
    assert!(mock.state.lock().expect("state").timer.is_some());
}

#[tokio::test]
async fn failed_activation_is_returned_even_when_timer_template_was_saved() {
    let mock = MockCommunity::start(false, false).await;
    mock.state.lock().expect("state").fail_start = true;
    let client = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .expect("login");
    let sync = TaskSync::new(client, "worker-1").await.expect("executor");
    assert!(
        sync.upsert_job(
            "task-1",
            "fixture",
            "http://platform/start",
            "fixture",
            Some("*/5 * * * *")
        )
        .await
        .is_err()
    );
    let state = mock.state.lock().expect("state");
    assert!(state.timer.is_some());
    assert!(state.runtime.is_none());
}

#[tokio::test]
async fn failed_worker_stop_prevents_template_deletion_and_false_success() {
    let mock = MockCommunity::start(false, false).await;
    let client = JiaSchedulerClient::login(&mock.url, "admin", "fixture-password")
        .await
        .expect("login");
    let sync = TaskSync::new(client, "worker-1").await.expect("executor");
    sync.upsert_job(
        "task-1",
        "fixture",
        "http://platform/start",
        "fixture",
        Some("*/5 * * * *"),
    )
    .await
    .expect("upsert");
    mock.state.lock().expect("state").fail_stop = true;
    assert!(sync.remove_timer("task-1").await.is_err());
    let state = mock.state.lock().expect("state");
    assert!(state.timer.is_some());
    assert_eq!(
        state.runtime.as_ref().expect("runtime")["action"],
        "start_timer"
    );
    assert!(
        !state
            .requests
            .iter()
            .any(|(path, _)| path == "/api/job/delete-timer")
    );
}
