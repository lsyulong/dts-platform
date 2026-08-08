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


use serde::{Deserialize, Serialize};

// ── SaveJob ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SaveJobReq {
    pub name: String,
    pub job_type: String,
    pub code: String,
    pub executor_id: u64,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub timeout: u64,
    #[serde(default)]
    pub max_retry: u8,
    #[serde(default)]
    pub max_parallel: u8,
    #[serde(default)]
    pub work_dir: String,
    #[serde(default)]
    pub work_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_callback: Option<CompletedCallbackOpts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CompletedCallbackOpts {
    pub url: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<CompletedCallbackHeader>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompletedCallbackHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveJobResp {
    pub result: u64, // job id
}

// ── SaveTimer ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SaveTimerReq {
    pub name: String,
    pub eid: String,
    pub job_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_expr: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_args: Option<serde_json::Value>,
    #[serde(default)]
    pub info: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveTimerResp {
    pub result: u64,
}

// ── Dispatch ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DispatchJobReq {
    pub eid: String,
    pub action: String,
    pub schedule_type: String,
    pub endpoints: Vec<DispatchEndpoint>,
    pub schedule_name: String,
    #[serde(default)]
    pub is_sync: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_expr: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct DispatchEndpoint {
    pub instance_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DispatchJobResp {
    pub result: u64,
}

// ── Query Timer ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct QueryTimerParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
}

#[allow(dead_code)]
fn default_page() -> u64 {
    1
}
#[allow(dead_code)]
fn default_page_size() -> u64 {
    100
}

#[derive(Debug, Deserialize)]
pub struct QueryJobTimerResp {
    pub data: Vec<JobTimerRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct JobTimerRecord {
    pub id: u64,
    pub eid: String,
    pub name: String,
    pub job_name: String,
    pub job_type: String,
    pub timer_expr: Option<serde_json::Value>,
    pub job_args: Option<serde_json::Value>,
    pub info: String,
    pub created_user: String,
    pub created_time: String,
    pub updated_time: String,
}

// ── Query Exec History ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QueryExecHistoryResp {
    pub data: Vec<ExecRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExecRecord {
    pub id: u64,
    pub eid: String,
    pub instance_id: String,
    pub schedule_id: String,
    pub exit_code: i32,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub output: Option<String>,
}

// ── Query Schedule History ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QueryScheduleHistoryResp {
    pub data: Vec<ScheduleHistoryRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleHistoryRecord {
    pub id: u64,
    pub name: String,
    pub eid: String,
    pub schedule_type: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub exit_code: Option<i32>,
}

// ── Delete Timer ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DeleteTimerReq {
    pub id: u64,
}

// ── Save Executor ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SaveExecutorReq {
    pub name: String,
    pub executor_type: String,
    #[serde(default)]
    pub info: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveExecutorResp {
    pub result: u64,
}

// ── Query Executor ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QueryExecutorResp {
    pub data: Vec<ExecutorRecord>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExecutorRecord {
    pub id: u64,
    pub name: String,
    pub executor_type: String,
}

// ── Dashboard Stats ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DashboardStats {
    pub today_total: u64,
    pub today_success: u64,
    pub today_failed: u64,
    pub today_running: u64,
}
