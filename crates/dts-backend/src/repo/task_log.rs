// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use serde::Serialize;

use crate::BackendError;

#[derive(Debug, Clone)]
pub struct AppendTaskLog {
    pub id: String,
    pub tenant_id: String,
    pub task_id: String,
    pub run_id: String,
    pub timestamp: String,
    pub level: String,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLogRecord {
    pub id: String,
    pub sequence: i64,
    pub timestamp: String,
    pub level: String,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLogPage {
    pub data: Vec<TaskLogRecord>,
    pub total: i64,
}

#[async_trait]
pub trait TaskLogRepo: Send + Sync {
    /// Append to an owned run and allocate its sequence atomically.
    async fn append(&self, params: &AppendTaskLog) -> Result<TaskLogRecord, BackendError>;
    /// Return NotFound for an absent or differently owned task/run, even if no logs exist.
    async fn list(
        &self,
        tenant_id: &str,
        task_id: &str,
        run_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<TaskLogPage, BackendError>;
}
