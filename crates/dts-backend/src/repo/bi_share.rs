// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::BackendError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod validation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BiShareStatus {
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BiShare {
    pub id: String,
    pub tenant_id: String,
    pub report_id: String,
    pub report_version: i64,
    pub job_id: String,
    pub node_id: String,
    pub owner_id: String,
    pub recipient_id: String,
    pub status: BiShareStatus,
    pub version: i64,
    pub expires_at: DateTime<Utc>,
    pub is_expired: bool,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub struct CreateBiShareParams {
    pub id: String,
    pub tenant_id: String,
    pub owner_id: String,
    pub recipient_id: String,
    pub report_id: String,
    pub report_version: i64,
    pub job_id: String,
    pub node_id: String,
    pub expires_at: DateTime<Utc>,
}
pub struct BiShareLookup {
    pub tenant_id: String,
    pub actor_id: String,
    pub share_id: String,
}
pub struct BiShareFilter {
    pub tenant_id: String,
    pub actor_id: String,
    pub report_id: Option<String>,
    pub limit: u32,
    pub offset: u32,
}
pub struct RevokeBiShareParams {
    pub tenant_id: String,
    pub actor_id: String,
    pub share_id: String,
    pub expected_version: i64,
}
#[derive(Debug, Serialize)]
pub struct BiSharePage {
    pub data: Vec<BiShare>,
    pub total: i64,
}

#[async_trait]
pub trait BiShareRepo: Send + Sync {
    /// Transactionally bind one real recipient to an actual successful BI node
    /// and currently published report version; both users must remain active.
    async fn create(&self, params: &CreateBiShareParams) -> Result<BiShare, BackendError>;
    async fn list(&self, filter: &BiShareFilter) -> Result<BiSharePage, BackendError>;
    async fn get(&self, lookup: &BiShareLookup) -> Result<Option<BiShare>, BackendError>;
    /// Check DB-clock expiry, current users/BI permissions and published source.
    /// This never grants data access or releases stored row results.
    async fn open(&self, lookup: &BiShareLookup) -> Result<BiShare, BackendError>;
    async fn revoke(&self, params: &RevokeBiShareParams) -> Result<BiShare, BackendError>;
}
