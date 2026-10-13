// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{
    BackendError,
    repo::product_resource::{ProductResource, ResourceWrite},
};

pub mod validation;

#[cfg(test)]
mod tests;

pub struct SaveMasterResourceParams {
    pub tenant_id: String,
    pub id: String,
    pub actor_id: String,
    pub expected_version: Option<i64>,
    pub fields: ResourceWrite,
}

pub struct MasterScope {
    pub tenant_id: String,
    pub id: String,
    pub actor_id: String,
    pub expected_version: i64,
}

#[derive(Debug, Serialize)]
pub struct MasterModelPublication {
    pub resource: ProductResource,
    pub model_version: i64,
    pub schema_snapshot: Value,
    pub physical_published: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum MasterRecordAction {
    Submit,
    Approve,
    Reject,
}

pub struct MasterRecordActionParams {
    pub scope: MasterScope,
    pub action: MasterRecordAction,
    pub reason: String,
}

pub struct MasterAllocateParams {
    pub scope: MasterScope,
    pub idempotency_key: String,
}

#[derive(Debug, Serialize)]
pub struct MasterCodeAllocation {
    pub id: String,
    pub tenant_id: String,
    pub rule_id: String,
    pub rule_version: i64,
    pub model_id: String,
    pub actor_id: String,
    pub idempotency_key: String,
    pub sequence: i64,
    pub code: String,
    pub created_at: DateTime<Utc>,
}

pub struct DeleteMasterResourceParams {
    pub scope: MasterScope,
    pub kind: String,
}

pub struct MasterApprovedFilter {
    pub tenant_id: String,
    pub model_id: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct MasterApprovedRecord {
    pub record_id: String,
    pub record_version: i64,
    pub model_id: String,
    pub model_version: i64,
    pub business_key: String,
    pub values: Value,
    pub approved_by: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MasterApprovedPage {
    pub data: Vec<MasterApprovedRecord>,
    pub total: i64,
}

#[async_trait]
pub trait MasterDataRepo: Send + Sync {
    async fn save_model(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError>;
    async fn publish_model(
        &self,
        scope: &MasterScope,
    ) -> Result<MasterModelPublication, BackendError>;
    async fn save_rule(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError>;
    async fn allocate_code(
        &self,
        params: &MasterAllocateParams,
    ) -> Result<MasterCodeAllocation, BackendError>;
    async fn save_record(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError>;
    async fn transition_record(
        &self,
        params: &MasterRecordActionParams,
    ) -> Result<ProductResource, BackendError>;
    async fn delete_draft(&self, params: &DeleteMasterResourceParams) -> Result<(), BackendError>;
    async fn approved_records(
        &self,
        filter: &MasterApprovedFilter,
    ) -> Result<MasterApprovedPage, BackendError>;
}
