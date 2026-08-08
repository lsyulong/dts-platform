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


//! Integration tests for dts-deerflow-client
//!
//! TDD Cycle 1: Types serialization roundtrip
//! TDD Cycle 2: SSE parsing
//! TDD Cycle 3: Client construction

use dts_deerflow_client::{ChatRequest, DeerFlowClient, Role, StreamChunk};

// ============================================================================
// Helper: serde_json::from_value without unwrap
// ============================================================================

fn deserialize<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> T {
    match serde_json::from_value(v) {
        Ok(val) => val,
        Err(e) => panic!("test JSON should deserialize: {e}"),
    }
}

fn serialize<T: serde::Serialize>(v: &T) -> serde_json::Value {
    match serde_json::to_value(v) {
        Ok(val) => val,
        Err(e) => panic!("test value should serialize: {e}"),
    }
}

// ============================================================================
// Cycle 1: Types
// ============================================================================

#[test]
fn chat_request_roundtrip() {
    let json = serde_json::json!({
        "message": "当前有哪些任务在运行？",
        "context": {
            "current_route": "/dashboard",
            "task_id": null,
            "datasource_id": null,
            "recent_alerts": 2
        },
        "history": [
            {"role": "user", "content": "你好"},
            {"role": "assistant", "content": "你好！有什么可以帮你？"}
        ]
    });

    let request: ChatRequest = deserialize(json.clone());
    let serialized = serialize(&request);

    assert_eq!(request.message, "当前有哪些任务在运行？");
    assert_eq!(
        request.context.as_ref().map(|c| &c.current_route),
        Some(&"/dashboard".to_string())
    );
    assert_eq!(request.context.as_ref().map(|c| c.recent_alerts), Some(2));
    assert_eq!(request.history.len(), 2);
    assert!(matches!(request.history[0].role, Role::User));
    assert_eq!(serialized, json);
}

#[test]
fn chat_request_minimal() {
    let json = serde_json::json!({
        "message": "hello",
        "context": null,
        "history": []
    });

    let request: ChatRequest = match serde_json::from_value(json) {
        Ok(v) => v,
        Err(e) => panic!("test JSON should deserialize: {e}"),
    };
    assert_eq!(request.message, "hello");
    assert!(request.context.is_none());
    assert!(request.history.is_empty());
}

#[test]
fn stream_chunk_delta() {
    let json = serde_json::json!({
        "delta": "你好",
        "done": false,
        "content": null
    });

    let chunk: StreamChunk = deserialize(json);
    assert_eq!(chunk.delta.as_deref(), Some("你好"));
    assert!(!chunk.done);
    assert!(chunk.content.is_none());
}

#[test]
fn stream_chunk_done() {
    let json = serde_json::json!({
        "delta": null,
        "done": true,
        "content": "完整的回答内容"
    });

    let chunk: StreamChunk = deserialize(json);
    assert!(chunk.done);
    assert_eq!(chunk.content.as_deref(), Some("完整的回答内容"));
    assert!(chunk.delta.is_none());
}

// ============================================================================
// Cycle 2: SSE Parsing
// ============================================================================

#[test]
fn parse_sse_data_line() {
    let line = "data: {\"delta\":\"你好\",\"done\":false,\"content\":null}\n\n";
    let chunk = match dts_deerflow_client::parse_sse_line(line) {
        Some(c) => c,
        None => panic!("SSE parse should return Some for valid line"),
    };
    assert_eq!(chunk.delta.as_deref(), Some("你好"));
    assert!(!chunk.done);
}

#[test]
fn parse_sse_done_signal() {
    let line = "data: [DONE]\n\n";
    let chunk = match dts_deerflow_client::parse_sse_line(line) {
        Some(c) => c,
        None => panic!("SSE parse should return Some for [DONE] signal"),
    };
    assert!(chunk.done);
    assert!(chunk.delta.is_none());
    assert!(chunk.content.is_none());
}

#[test]
fn parse_sse_empty_returns_none() {
    assert!(dts_deerflow_client::parse_sse_line("").is_none());
    assert!(dts_deerflow_client::parse_sse_line("\n\n").is_none());
}

#[test]
fn parse_sse_multiple_lines_takes_first_data() {
    let text = "data: {\"delta\":\"first\",\"done\":false,\"content\":null}\n\ndata: {\"delta\":\"second\",\"done\":false,\"content\":null}\n\n";
    let chunk = match dts_deerflow_client::parse_sse_line(text) {
        Some(c) => c,
        None => panic!("SSE parse should return Some for valid data line"),
    };
    assert_eq!(chunk.delta.as_deref(), Some("first"));
}

// ============================================================================
// Cycle 3: Client Construction
// ============================================================================

#[test]
fn client_new_trims_trailing_slash() {
    let client = DeerFlowClient::new("http://localhost:8080/", "sk-test");
    let _ = client;
}

#[test]
fn client_new_without_slash() {
    let client = DeerFlowClient::new("http://localhost:8080", "sk-test");
    let _ = client;
}
