// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::BackendError;

/// Internal notification connection record. `secret` is deliberately not serializable.
#[derive(Debug, Clone)]
pub struct NotificationConnection {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub channel: String,
    pub public_config: Value,
    pub secret: Option<String>,
    pub has_secret: bool,
    pub enabled: bool,
    pub version: i64,
    pub last_test_status: Option<String>,
    pub last_test_error: Option<String>,
    pub last_test_at: Option<DateTime<Utc>>,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateNotificationConnectionParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub channel: String,
    pub public_config: Value,
    pub secret: Option<String>,
    pub enabled: bool,
    pub actor_id: String,
}

#[derive(Debug, Clone)]
pub struct UpdateNotificationConnectionParams {
    pub tenant_id: String,
    pub id: String,
    pub expected_version: i64,
    pub name: String,
    pub public_config: Value,
    /// False preserves the stored secret. True replaces it, including clearing it with None.
    pub replace_secret: bool,
    pub secret: Option<String>,
    pub actor_id: String,
}

#[derive(Debug, Clone)]
pub struct SetNotificationConnectionEnabledParams {
    pub tenant_id: String,
    pub id: String,
    pub expected_version: i64,
    pub enabled: bool,
    pub actor_id: String,
}

#[derive(Debug, Clone)]
pub struct RecordNotificationConnectionTestParams {
    pub tenant_id: String,
    pub id: String,
    pub status: String,
    pub error: Option<String>,
    pub actor_id: String,
    pub tested_at: DateTime<Utc>,
}

#[async_trait]
pub trait NotificationConnectionRepo: Send + Sync {
    async fn create(
        &self,
        params: &CreateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError>;
    async fn list(&self, tenant_id: &str) -> Result<Vec<NotificationConnection>, BackendError>;
    async fn get(
        &self,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<NotificationConnection>, BackendError>;
    async fn update(
        &self,
        params: &UpdateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError>;
    async fn set_enabled(
        &self,
        params: &SetNotificationConnectionEnabledParams,
    ) -> Result<NotificationConnection, BackendError>;
    async fn delete(
        &self,
        tenant_id: &str,
        id: &str,
        expected_version: i64,
        actor_id: &str,
    ) -> Result<bool, BackendError>;
    async fn record_test(
        &self,
        params: &RecordNotificationConnectionTestParams,
    ) -> Result<(), BackendError>;
}
