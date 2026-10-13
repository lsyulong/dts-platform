// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagAssignment {
    pub id: String,
    pub tenant_id: String,
    pub tag_id: String,
    pub tag_version: i64,
    pub target_kind: String,
    pub target_id: String,
    pub target_code: String,
    pub target_name: String,
    pub assigned_by: String,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagAssignmentMutation {
    pub assignment: TagAssignment,
    pub active: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagAssignmentPage {
    pub data: Vec<TagAssignment>,
    pub total: i64,
}

pub struct TagAssignmentCommand {
    pub tenant_id: String,
    pub actor_id: String,
    pub tag_id: String,
    pub expected_tag_version: i64,
    pub target_kind: String,
    pub target_id: String,
    pub idempotency_key: String,
}

pub struct TagAssignmentFilter {
    pub tenant_id: String,
    pub tag_id: String,
    pub limit: u32,
    pub offset: u32,
}

/// Durable tag relationships. Each mutation validates both resources, writes an
/// immutable idempotency receipt and changes the active relationship atomically.
#[async_trait]
pub trait TagAssignmentRepo: Send + Sync {
    async fn assign(
        &self,
        command: &TagAssignmentCommand,
    ) -> Result<TagAssignmentMutation, BackendError>;
    async fn unassign(
        &self,
        command: &TagAssignmentCommand,
    ) -> Result<TagAssignmentMutation, BackendError>;
    async fn list(&self, filter: &TagAssignmentFilter) -> Result<TagAssignmentPage, BackendError>;
}

pub fn validate_command(
    command: &TagAssignmentCommand,
    action: &str,
) -> Result<String, BackendError> {
    if !matches!(action, "assign" | "unassign")
        || command.expected_tag_version < 1
        || command.target_kind == "tag"
        || command.target_kind.is_empty()
        || command.target_kind.len() > 128
        || command.target_id.is_empty()
        || command.target_id.len() > 128
        || command.idempotency_key.is_empty()
        || command.idempotency_key.len() > 128
        || [
            &command.tenant_id,
            &command.actor_id,
            &command.tag_id,
            &command.target_kind,
            &command.target_id,
            &command.idempotency_key,
        ]
        .iter()
        .any(|value| value.trim() != value.as_str() || value.chars().any(char::is_control))
    {
        return Err(BackendError::ConstraintViolation("标签绑定请求无效".into()));
    }
    let semantic = serde_json::to_vec(&serde_json::json!({
        "action": action,
        "tenant_id": command.tenant_id,
        "actor_id": command.actor_id,
        "tag_id": command.tag_id,
        "expected_tag_version": command.expected_tag_version,
        "target_kind": command.target_kind,
        "target_id": command.target_id,
    }))
    .map_err(|error| BackendError::Internal(format!("标签绑定请求序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(semantic)))
}
