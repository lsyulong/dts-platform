// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use dts_backend::{
    BackendError,
    repo::{master_data::*, product_resource::ProductResource},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

mod models;
mod records;
mod sequence;
mod storage;

pub struct SqliteMasterDataRepo {
    pool: SqlitePool,
}

impl SqliteMasterDataRepo {
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
        let active: Option<String> = sqlx::query_scalar(
            "SELECT u.id FROM \"user\" u JOIN tenant t ON u.tenant_id = t.id
            WHERE t.id = ?1 AND t.status = 'active' AND u.id = ?2 AND u.status = 'active'",
        )
        .bind(tenant)
        .bind(actor)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage::error)?;
        if active.is_none() {
            return Err(BackendError::ConstraintViolation(
                "租户或操作账号不属于当前范围或已停用".into(),
            ));
        }
        Ok(tx)
    }
}

#[async_trait]
impl MasterDataRepo for SqliteMasterDataRepo {
    async fn save_model(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError> {
        models::save(self, params).await
    }
    async fn publish_model(
        &self,
        scope: &MasterScope,
    ) -> Result<MasterModelPublication, BackendError> {
        models::publish(self, scope).await
    }
    async fn save_rule(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError> {
        sequence::save(self, params).await
    }
    async fn allocate_code(
        &self,
        params: &MasterAllocateParams,
    ) -> Result<MasterCodeAllocation, BackendError> {
        sequence::allocate(self, params).await
    }
    async fn save_record(
        &self,
        params: &SaveMasterResourceParams,
    ) -> Result<ProductResource, BackendError> {
        records::save(self, params).await
    }
    async fn transition_record(
        &self,
        params: &MasterRecordActionParams,
    ) -> Result<ProductResource, BackendError> {
        records::transition(self, params).await
    }
    async fn delete_draft(&self, params: &DeleteMasterResourceParams) -> Result<(), BackendError> {
        models::delete(self, params).await
    }
    async fn approved_records(
        &self,
        filter: &MasterApprovedFilter,
    ) -> Result<MasterApprovedPage, BackendError> {
        records::approved(self, filter).await
    }
}
