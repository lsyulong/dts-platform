// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::BackendError;

pub mod validation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    ReconciliationRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductJobNodeStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    ReconciliationRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductJobNodeDefinition {
    pub id: String,
    pub name: String,
    pub definition: Value,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductJobNode {
    pub id: String,
    pub name: String,
    pub definition: Value,
    pub depends_on: Vec<String>,
    pub status: ProductJobNodeStatus,
    pub attempt: i64,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductJob {
    pub id: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub actor_id: String,
    pub action: String,
    pub idempotency_key: String,
    pub definition_snapshot: Value,
    pub parameters: Value,
    pub status: ProductJobStatus,
    pub cancel_requested: bool,
    pub cancel_requested_by: Option<String>,
    pub can_retry: bool,
    pub attempt: i64,
    pub lease_owner: Option<String>,
    pub lease_fence: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub reconciled_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub nodes: Vec<ProductJobNode>,
}

#[derive(Debug, Clone)]
pub struct EnqueueProductJobParams {
    pub id: String,
    pub tenant_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_version: i64,
    pub actor_id: String,
    pub action: String,
    pub idempotency_key: String,
    pub definition_snapshot: Value,
    pub parameters: Value,
    /// Set only by an application executor after validating the entire job as read-only.
    pub can_retry: bool,
    pub nodes: Vec<ProductJobNodeDefinition>,
}

pub struct EnqueuedProductJob {
    pub job: ProductJob,
    pub created: bool,
}

pub struct ProductJobFilter {
    pub tenant_id: String,
    pub actor_id: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub status: Option<ProductJobStatus>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct ProductJobPage {
    pub data: Vec<ProductJob>,
    pub total: i64,
}

pub struct ProductJobLookup {
    pub tenant_id: String,
    pub job_id: String,
    pub actor_id: Option<String>,
}

pub struct ClaimProductJobParams {
    pub tenant_id: String,
    pub worker_id: String,
    pub source_kinds: Vec<String>,
    pub lease_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct ProductJobLease {
    pub worker_id: String,
    pub fence: i64,
}

pub struct HeartbeatProductJobParams {
    pub tenant_id: String,
    pub job_id: String,
    pub lease: ProductJobLease,
    pub lease_seconds: u32,
}

pub struct StartProductJobNodeParams {
    pub tenant_id: String,
    pub job_id: String,
    pub node_id: String,
    pub lease: ProductJobLease,
}

#[derive(Debug, Clone, Copy)]
pub enum ProductJobOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

pub struct FinishProductJobNodeParams {
    pub tenant_id: String,
    pub job_id: String,
    pub node_id: String,
    pub lease: ProductJobLease,
    pub outcome: ProductJobOutcome,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub struct FinishProductJobParams {
    pub tenant_id: String,
    pub job_id: String,
    pub lease: ProductJobLease,
    pub outcome: ProductJobOutcome,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub struct CancelProductJobParams {
    pub tenant_id: String,
    pub job_id: String,
    pub actor_id: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ProductJobReconciliation {
    Failed,
    Cancelled,
}

pub struct ReconcileProductJobParams {
    pub tenant_id: String,
    pub job_id: String,
    pub actor_id: String,
    pub expected_fence: i64,
    pub outcome: ProductJobReconciliation,
    pub reason: String,
}

/// Durable control-plane execution state. SQL execution and authorization stay
/// in application services. Lease fencing applies to every executor mutation.
#[async_trait]
pub trait ProductJobRepo: Send + Sync {
    async fn enqueue(
        &self,
        params: &EnqueueProductJobParams,
    ) -> Result<EnqueuedProductJob, BackendError>;
    async fn list(&self, filter: &ProductJobFilter) -> Result<ProductJobPage, BackendError>;
    async fn get(&self, lookup: &ProductJobLookup) -> Result<Option<ProductJob>, BackendError>;
    async fn claim(
        &self,
        params: &ClaimProductJobParams,
    ) -> Result<Option<ProductJob>, BackendError>;
    async fn heartbeat(
        &self,
        params: &HeartbeatProductJobParams,
    ) -> Result<ProductJob, BackendError>;
    async fn start_node(
        &self,
        params: &StartProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError>;
    async fn finish_node(
        &self,
        params: &FinishProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError>;
    async fn finish(&self, params: &FinishProductJobParams) -> Result<ProductJob, BackendError>;
    async fn request_cancel(
        &self,
        params: &CancelProductJobParams,
    ) -> Result<ProductJob, BackendError>;
    async fn reconcile(
        &self,
        params: &ReconcileProductJobParams,
    ) -> Result<ProductJob, BackendError>;
}
