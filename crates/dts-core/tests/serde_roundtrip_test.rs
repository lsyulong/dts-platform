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


//! Serde round-trip tests for shared types in dts-core
//!
//! Verifies that all serializable types can be serialized to JSON
//! and deserialized back without data loss.

use dts_core::*;
use pretty_assertions::assert_eq;
use serde_json;

// ========== TenantInfo ==========

#[test]
fn tenant_info_roundtrip() {
    let input = TenantInfo {
        id: "t-001".into(),
        name: "Acme Corp".into(),
        slug: "acme".into(),
        status: "active".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: TenantInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.name, input.name);
    assert_eq!(output.slug, input.slug);
    assert_eq!(output.status, input.status);
}

#[test]
fn tenant_info_json_structure() {
    let tenant = TenantInfo {
        id: "t-001".into(),
        name: "Test".into(),
        slug: "test".into(),
        status: "active".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&tenant).unwrap();

    assert!(json["id"].is_string());
    assert!(json["name"].is_string());
    assert!(json["slug"].is_string());
    assert!(json["status"].is_string());
    assert!(json["created_at"].is_string());

    // Verify it deserializes into the expected shape
    assert_eq!(json["name"].as_str().unwrap(), "Test");
    assert_eq!(json["status"].as_str().unwrap(), "active");
}

// ========== DatasourceInfo ==========

#[test]
fn datasource_info_roundtrip() {
    let input = DatasourceInfo {
        id: "ds-1".into(),
        name: "Production MySQL".into(),
        db_type: "mysql".into(),
        host: "db.example.com".into(),
        port: 3306,
        username: Some("root".into()),
        database_name: Some("mydb".into()),
        health_status: "healthy".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: DatasourceInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.name, input.name);
    assert_eq!(output.db_type, input.db_type);
    assert_eq!(output.host, input.host);
    assert_eq!(output.port, input.port);
    assert_eq!(output.username, input.username);
    assert_eq!(output.database_name, input.database_name);
    assert_eq!(output.health_status, input.health_status);
}

#[test]
fn datasource_info_with_none_fields() {
    let input = DatasourceInfo {
        id: "ds-min".into(),
        name: "Minimal DB".into(),
        db_type: "sqlite".into(),
        host: "localhost".into(),
        port: 0,
        username: None,
        database_name: None,
        health_status: "unknown".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&input).unwrap();
    let output: DatasourceInfo = serde_json::from_value(json).unwrap();

    assert_eq!(output.username, None);
    assert_eq!(output.database_name, None);
    assert_eq!(output.health_status, "unknown");
}

// ========== DatasourceDetail ==========

#[test]
fn datasource_detail_roundtrip() {
    let input = DatasourceDetail {
        id: "dd-1".into(),
        name: "Full Detail DS".into(),
        db_type: "postgresql".into(),
        host: "pg.example.com".into(),
        port: 5432,
        username: Some("postgres".into()),
        password_masked: Some("****".into()),
        database_name: Some("analytics".into()),
        extra_params: Some(r#"{"sslmode":"require"}"#.into()),
        health_status: "healthy".into(),
        last_test_at: Some("2026-01-01T00:00:00Z".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-06-01T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: DatasourceDetail = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.name, input.name);
    assert_eq!(output.db_type, input.db_type);
    assert_eq!(output.port, input.port);
    assert_eq!(output.password_masked, Some("****".into()));
    assert_eq!(output.extra_params, Some(r#"{"sslmode":"require"}"#.into()));
    assert_eq!(output.last_test_at, Some("2026-01-01T00:00:00Z".into()));
}

// ========== TaskInfo ==========

#[test]
fn task_info_roundtrip() {
    let input = TaskInfo {
        id: "task-1".into(),
        name: "Sync Users Table".into(),
        description: Some("Sync users from MySQL to PG".into()),
        source_id: "ds-source".into(),
        target_id: "ds-target".into(),
        cron_expr: Some("0 */6 * * *".into()),
        status: "running".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: TaskInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.name, input.name);
    assert_eq!(output.description, input.description);
    assert_eq!(output.source_id, input.source_id);
    assert_eq!(output.target_id, input.target_id);
    assert_eq!(output.cron_expr, input.cron_expr);
    assert_eq!(output.status, input.status);
}

#[test]
fn task_info_minimal_fields() {
    let input = TaskInfo {
        id: "task-min".into(),
        name: "Minimal Task".into(),
        description: None,
        source_id: "s1".into(),
        target_id: "t1".into(),
        cron_expr: None,
        status: "draft".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&input).unwrap();
    let output: TaskInfo = serde_json::from_value(json).unwrap();

    assert_eq!(output.description, None);
    assert_eq!(output.cron_expr, None);
    assert_eq!(output.status, "draft");
}

// ========== TaskDetail ==========

#[test]
fn task_detail_roundtrip() {
    let input = TaskDetail {
        id: "td-1".into(),
        name: "Detailed Task".into(),
        description: Some("Full config task".into()),
        source_id: "src-1".into(),
        target_id: "tgt-1".into(),
        source_name: "MySQL Prod".into(),
        source_type: "mysql".into(),
        target_name: "PG Analytics".into(),
        target_type: "postgresql".into(),
        cron_expr: Some("0 0 * * *".into()),
        status: "completed".into(),
        config_json: r#"{"tables":["users"]}"#.into(),
        created_by: "admin".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-06-01T00:00:00Z".into(),
        recent_runs: vec![],
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: TaskDetail = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.source_name, input.source_name);
    assert_eq!(output.target_name, input.target_name);
    assert_eq!(output.config_json, input.config_json);
    assert_eq!(output.created_by, input.created_by);
    assert_eq!(output.recent_runs.len(), 0);
}

#[test]
fn task_detail_with_recent_runs() {
    let run = TaskRunInfo {
        id: "run-1".into(),
        status: "completed".into(),
        start_time: "2026-01-01T08:00:00Z".into(),
        end_time: Some("2026-01-01T08:05:00Z".into()),
        rows_synced: 10000,
        bytes_synced: 5000000,
        total_rows: Some(10000),
        total_bytes: Some(5000000),
        error_message: None,
    };

    let input = TaskDetail {
        id: "td-with-runs".into(),
        name: "Task With Runs".into(),
        description: None,
        source_id: "src".into(),
        target_id: "tgt".into(),
        source_name: "Src".into(),
        source_type: "mysql".into(),
        target_name: "Tgt".into(),
        target_type: "pg".into(),
        cron_expr: None,
        status: "completed".into(),
        config_json: "{}".into(),
        created_by: "admin".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        recent_runs: vec![run],
    };

    let json = serde_json::to_string(&input).unwrap();
    let output: TaskDetail = serde_json::from_str(&json).unwrap();

    assert_eq!(output.recent_runs.len(), 1);
    let first_run = &output.recent_runs[0];
    assert_eq!(first_run.status, "completed");
    assert_eq!(first_run.rows_synced, 10000);
    assert_eq!(first_run.bytes_synced, 5000000);
}

// ========== TaskRunInfo ==========

#[test]
fn task_run_info_roundtrip() {
    let input = TaskRunInfo {
        id: "run-1".into(),
        status: "running".into(),
        start_time: "2026-01-01T08:00:00Z".into(),
        end_time: None,
        rows_synced: 5000,
        bytes_synced: 2500000,
        total_rows: Some(50000),
        total_bytes: Some(25000000),
        error_message: None,
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: TaskRunInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.status, input.status);
    assert_eq!(output.rows_synced, 5000);
    assert_eq!(output.bytes_synced, 2500000);
    assert_eq!(output.total_rows, Some(50000));
    assert_eq!(output.total_bytes, Some(25000000));
    assert_eq!(output.end_time, None);
    assert_eq!(output.error_message, None);
}

#[test]
fn task_run_info_with_error() {
    let input = TaskRunInfo {
        id: "run-failed".into(),
        status: "failed".into(),
        start_time: "2026-01-01T08:00:00Z".into(),
        end_time: Some("2026-01-01T08:02:00Z".into()),
        rows_synced: 100,
        bytes_synced: 50000,
        total_rows: Some(50000),
        total_bytes: Some(25000000),
        error_message: Some("Connection timeout".into()),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: TaskRunInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.status, "failed");
    assert_eq!(output.error_message, Some("Connection timeout".into()));
    assert_eq!(output.end_time, Some("2026-01-01T08:02:00Z".into()));
}

// ========== AlertInfo ==========

#[test]
fn alert_info_roundtrip() {
    let input = AlertInfo {
        id: "alert-1".into(),
        name: "High Error Rate".into(),
        metric_type: "error_count".into(),
        operator: "gt".into(),
        threshold: 10.0,
        enabled: true,
        notify_channels: r#"["email","webhook"]"#.into(),
        task_id: Some("task-1".into()),
        task_name: Some("Sync Users".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: AlertInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(output.id, input.id);
    assert_eq!(output.name, input.name);
    assert_eq!(output.metric_type, input.metric_type);
    assert_eq!(output.operator, input.operator);
    assert!((output.threshold - 10.0).abs() < f64::EPSILON);
    assert!(output.enabled);
    assert_eq!(output.task_id, Some("task-1".into()));
    assert_eq!(output.task_name, Some("Sync Users".into()));
}

#[test]
fn alert_info_disabled() {
    let input = AlertInfo {
        id: "alert-disabled".into(),
        name: "Disabled Alert".into(),
        metric_type: "sync_delay".into(),
        operator: "gte".into(),
        threshold: 300.0,
        enabled: false,
        notify_channels: "[]".into(),
        task_id: None,
        task_name: None,
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let json = serde_json::to_value(&input).unwrap();
    assert_eq!(json["enabled"], false);
    assert!(json["task_id"].is_null());
    assert!(json["task_name"].is_null());
}

// ========== MonitoringOverview ==========

#[test]
fn monitoring_overview_roundtrip() {
    let input = MonitoringOverview {
        total_tasks: 25,
        running_tasks: 5,
        failed_tasks: 2,
        total_datasources: 10,
        healthy_datasources: 9,
        today_synced_rows: 1_000_000,
        today_synced_bytes: 50_000_000,
        success_rate: 92.0,
        recent_runs: vec![],
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: MonitoringOverview = serde_json::from_str(&json).unwrap();

    assert_eq!(output.total_tasks, 25);
    assert_eq!(output.running_tasks, 5);
    assert_eq!(output.failed_tasks, 2);
    assert_eq!(output.total_datasources, 10);
    assert_eq!(output.healthy_datasources, 9);
    assert_eq!(output.today_synced_rows, 1_000_000);
    assert_eq!(output.today_synced_bytes, 50_000_000);
    assert!((output.success_rate - 92.0).abs() < f64::EPSILON);
    assert!(output.recent_runs.is_empty());
}

#[test]
fn monitoring_overview_with_recent_runs() {
    let run = TaskRunInfo {
        id: "r1".into(),
        status: "completed".into(),
        start_time: "2026-01-01T08:00:00Z".into(),
        end_time: Some("2026-01-01T08:05:00Z".into()),
        rows_synced: 50000,
        bytes_synced: 25000000,
        total_rows: Some(50000),
        total_bytes: Some(25000000),
        error_message: None,
    };

    let input = MonitoringOverview {
        total_tasks: 1,
        running_tasks: 0,
        failed_tasks: 0,
        total_datasources: 2,
        healthy_datasources: 2,
        today_synced_rows: 50000,
        today_synced_bytes: 25000000,
        success_rate: 100.0,
        recent_runs: vec![run],
    };
    let json = serde_json::to_string(&input).unwrap();
    let output: MonitoringOverview = serde_json::from_str(&json).unwrap();

    assert_eq!(output.recent_runs.len(), 1);
    assert_eq!(output.recent_runs[0].status, "completed");
    assert_eq!(output.recent_runs[0].rows_synced, 50000);
}

#[test]
fn monitoring_overview_default_values() {
    let overview = MonitoringOverview {
        total_tasks: 0,
        running_tasks: 0,
        failed_tasks: 0,
        total_datasources: 0,
        healthy_datasources: 0,
        today_synced_rows: 0,
        today_synced_bytes: 0,
        success_rate: 100.0,
        recent_runs: vec![],
    };

    let json = serde_json::to_value(&overview).unwrap();
    assert_eq!(json["total_tasks"], 0);
    assert_eq!(json["success_rate"], 100.0);
    assert!(json["recent_runs"].as_array().unwrap().is_empty());
}
