// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dts_backend::{
    BackendError,
    repo::task_log::{AppendTaskLog, TaskLogPage, TaskLogRecord, TaskLogRepo},
};

#[derive(Clone, Copy)]
pub struct RunLogScope<'a> {
    pub tenant_id: &'a str,
    pub task_id: &'a str,
    pub run_id: &'a str,
}

#[derive(Clone)]
pub struct TaskLogService {
    repo: Arc<dyn TaskLogRepo>,
}
impl TaskLogService {
    pub fn new(repo: Arc<dyn TaskLogRepo>) -> Self {
        Self { repo }
    }
    pub async fn record(
        &self,
        scope: RunLogScope<'_>,
        level: &str,
        component: &str,
        message: &str,
    ) -> Result<TaskLogRecord, BackendError> {
        let level = level.to_ascii_uppercase();
        if !["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"].contains(&level.as_str())
            || component.is_empty()
            || component.len() > 64
            || message.len() > 8192
        {
            return Err(BackendError::ConstraintViolation("运行日志格式无效".into()));
        }
        self.repo
            .append(&AppendTaskLog {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: scope.tenant_id.into(),
                task_id: scope.task_id.into(),
                run_id: scope.run_id.into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                level,
                component: component.into(),
                message: message.into(),
            })
            .await
    }
    pub async fn list(
        &self,
        scope: RunLogScope<'_>,
        offset: i64,
        limit: i64,
    ) -> Result<TaskLogPage, BackendError> {
        self.repo
            .list(scope.tenant_id, scope.task_id, scope.run_id, offset, limit)
            .await
    }
}

/// Only adapter messages known to exclude raw connection details are persisted verbatim.
pub fn safe_engine_message(message: &str) -> String {
    const FIXED: [&str; 8] = [
        "任务已取消",
        "引擎取消通道意外断开",
        "ape-dts task timed out (1 hour)",
        "全局计数来自最后的日志采样，需通过数据对账确认最终结果",
        "执行已结束，未获得完整的全局计数；需进行数据对账",
        "引擎进度通道在终态之前关闭",
        "引擎进度标识不匹配",
        "引擎返回不一致的 PipelineRun ID",
    ];
    let counters = message
        .strip_prefix("已观察到全局写入计数：")
        .and_then(|value| value.split_once(" 行，"))
        .and_then(|(rows, bytes)| bytes.strip_suffix(" 字节").map(|bytes| (rows, bytes)))
        .is_some_and(|(rows, bytes)| rows.parse::<u64>().is_ok() && bytes.parse::<u64>().is_ok());
    let exit = message
        .strip_prefix("ape-dts exited with code ")
        .is_some_and(|value| {
            value == "None"
                || value
                    .strip_prefix("Some(")
                    .and_then(|value| value.strip_suffix(')'))
                    .is_some_and(|code| code.parse::<i32>().is_ok())
        });
    if FIXED.contains(&message) || counters || exit {
        message.to_owned()
    } else {
        "引擎诊断内容已保护，请检查受控服务日志".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_actual_safe_counters_and_exit_codes_but_not_connection_details() {
        let counter = "已观察到全局写入计数：125 行，2048 字节";
        assert_eq!(safe_engine_message(counter), counter);
        assert_eq!(
            safe_engine_message("ape-dts exited with code Some(1)"),
            "ape-dts exited with code Some(1)"
        );
        for text in [
            "postgres://user:secret@db",
            "password=secret",
            "Bearer session-secret",
            "已观察到全局写入计数：1 行，token secret",
            "unknown adapter raw diagnostic",
        ] {
            let safe = safe_engine_message(text);
            assert!(!safe.contains("secret"));
            assert_ne!(safe, text);
        }
    }
}
