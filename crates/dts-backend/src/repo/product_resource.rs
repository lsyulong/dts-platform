// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::BackendError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReference {
    pub field: String,
    pub target_kind: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductResource {
    pub id: String,
    pub tenant_id: String,
    pub kind: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub status: String,
    pub properties: Value,
    pub version: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub references: Vec<ResourceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceWrite {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub status: String,
    pub properties: Value,
    pub references: Vec<ResourceReference>,
}

pub struct CreateResourceParams {
    pub id: String,
    pub tenant_id: String,
    pub kind: String,
    pub actor_id: String,
    pub fields: ResourceWrite,
}

pub struct UpdateResourceParams {
    pub expected_version: i64,
    pub actor_id: String,
    pub fields: ResourceWrite,
}

pub struct ProductResourceFilter {
    pub tenant_id: String,
    pub kind: String,
    pub q: Option<String>,
    pub status: Option<String>,
    pub created_by: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct ResourcePage {
    pub data: Vec<ProductResource>,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct ResourceAudit {
    pub id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub operation: String,
    pub summary: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ResourceAuditPage {
    pub data: Vec<ResourceAudit>,
    pub total: i64,
}

/// Tenant-scoped configuration storage. Domain schemas and allowed actions belong
/// to application services; graph integrity, version checks and audit are atomic.
#[async_trait]
pub trait ProductResourceRepo: Send + Sync {
    async fn list(&self, filter: &ProductResourceFilter) -> Result<ResourcePage, BackendError>;
    async fn get(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<ProductResource>, BackendError>;
    async fn create(&self, params: &CreateResourceParams) -> Result<ProductResource, BackendError>;
    async fn update(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
        params: &UpdateResourceParams,
    ) -> Result<ProductResource, BackendError>;
    async fn delete(
        &self,
        tenant_id: &str,
        kind: &str,
        id: &str,
        expected_version: i64,
        actor_id: &str,
    ) -> Result<(), BackendError>;
    async fn list_audit(
        &self,
        tenant_id: &str,
        kind: Option<&str>,
        resource_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourceAuditPage, BackendError>;
}
