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

// ============================================================
// 租户
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateTenantParams {
    pub id: String,
    pub name: String,
    pub slug: String,
}

// ============================================================
// 用户
// ============================================================

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CreateUserParams {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
}

// ============================================================
// 角色 & 权限
// ============================================================

#[derive(Debug, Clone)]
pub struct CreateRoleParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub is_system: bool,
    pub permissions: Vec<String>,
}

/// 用户列表项（含租户名和角色名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub tenant_id: String,
    pub tenant_name: String,
    pub roles: Vec<String>,
    pub status: String,
    pub created_at: String,
}

/// 角色列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleListInfo {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub description: String,
    pub is_system: bool,
    pub permissions: Vec<String>,
    pub created_at: String,
}

/// 更新用户参数
#[derive(Debug, Clone)]
pub struct UpdateUserParams {
    pub display_name: Option<String>,
    pub status: Option<String>,
}

/// 更新角色参数
#[derive(Debug, Clone)]
pub struct UpdateRoleParams {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

/// 更新租户参数
#[derive(Debug, Clone)]
pub struct UpdateTenantParams {
    pub name: Option<String>,
    pub status: Option<String>,
}

// ============================================================
// 数据源
// ============================================================

#[derive(Debug, Clone)]
pub struct CreateDatasourceParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub encrypted_password: Option<String>,
    pub database_name: Option<String>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceInfo {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub database_name: Option<String>,
    pub health_status: String,
    pub last_test_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceDetail {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password_masked: Option<String>,
    pub database_name: Option<String>,
    pub extra_params: Option<String>,
    pub health_status: String,
    pub last_test_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================
// 同步任务
// ============================================================

#[derive(Debug, Clone)]
pub struct CreateTaskParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub config_json: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub source_name: String,
    pub source_type: String,
    pub target_name: String,
    pub target_type: String,
    pub cron_expr: Option<String>,
    pub status: String,
    pub config_json: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub recent_runs: Vec<TaskRunInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunInfo {
    pub id: String,
    pub status: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub rows_synced: i64,
    pub bytes_synced: i64,
    pub total_rows: Option<i64>,
    pub total_bytes: Option<i64>,
    pub error_message: Option<String>,
}

// ============================================================
// 告警
// ============================================================

#[derive(Debug, Clone)]
pub struct CreateAlertParams {
    pub id: String,
    pub tenant_id: String,
    pub task_id: Option<String>,
    pub name: String,
    pub metric_type: String,
    pub operator: String,
    pub threshold: f64,
    pub notify_channels: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertInfo {
    pub id: String,
    pub name: String,
    pub metric_type: String,
    pub operator: String,
    pub threshold: f64,
    pub enabled: bool,
    pub notify_channels: String,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    pub created_at: String,
}

// ============================================================
// 监控
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringOverview {
    pub total_tasks: i64,
    pub running_tasks: i64,
    pub failed_tasks: i64,
    pub total_datasources: i64,
    pub healthy_datasources: i64,
    pub today_synced_rows: i64,
    pub today_synced_bytes: i64,
    pub success_rate: f64,
    pub recent_runs: Vec<TaskRunInfo>,
}
