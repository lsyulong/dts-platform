// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, ProductResource, ResourcePage, repo::data_access::*};
use sqlx::{Sqlite, SqlitePool, Transaction};

mod audit;
pub(crate) mod authority;
mod decisions;
pub(crate) mod grants;
mod receipts;
mod requests;
pub(crate) mod storage;

pub struct SqliteDataAccessRepo {
    pool: SqlitePool,
}
impl SqliteDataAccessRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    async fn begin(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<Transaction<'_, Sqlite>, BackendError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage::error)?;
        let authority = authority::get(&mut tx, tenant, actor, true).await?;
        if !authority.active {
            return Err(BackendError::ConstraintViolation(
                "租户或当前账号已失效".into(),
            ));
        }
        Ok(tx)
    }
    async fn read_begin(&self) -> Result<Transaction<'_, Sqlite>, BackendError> {
        self.pool.begin().await.map_err(storage::error)
    }
}

#[async_trait]
impl DataAccessRepo for SqliteDataAccessRepo {
    async fn actor_authority(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<DataAccessAuthority, BackendError> {
        let mut tx = self.read_begin().await?;
        let result = authority::get(&mut tx, tenant_id, actor_id, false).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn save_request(
        &self,
        params: &SaveDataAccessRequestParams,
    ) -> Result<ProductResource, BackendError> {
        requests::save(self, params).await
    }
    async fn get_request(
        &self,
        tenant_id: &str,
        request_id: &str,
    ) -> Result<Option<DataAccessRequest>, BackendError> {
        let mut tx = self.read_begin().await?;
        let result = requests::get(&mut tx, tenant_id, request_id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(result)
    }
    async fn decide(
        &self,
        params: &DecideDataAccessParams,
    ) -> Result<DataAccessDecisionResult, BackendError> {
        decisions::decide(self, params).await
    }
    async fn list_requests(
        &self,
        filter: &DataAccessRequestFilter,
    ) -> Result<ResourcePage, BackendError> {
        requests::list(self, filter).await
    }
    async fn active_grants(
        &self,
        tenant_id: &str,
        actor_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<DataAccessGrant>, BackendError> {
        grants::active(self, tenant_id, actor_id, now).await
    }
    async fn record_access(&self, params: &RecordDataAccessParams) -> Result<(), BackendError> {
        audit::record(self, params).await
    }
    async fn list_audit(
        &self,
        filter: &DataAccessAuditFilter,
    ) -> Result<DataAccessAuditPage, BackendError> {
        audit::list(self, filter).await
    }
}
