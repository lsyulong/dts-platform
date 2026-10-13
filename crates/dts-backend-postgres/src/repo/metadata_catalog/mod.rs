// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use dts_backend::{BackendError, repo::metadata_catalog::*};
use sqlx::PgPool;

mod reflect;
mod storage;

pub struct PgMetadataCatalogRepo {
    pool: PgPool,
}

impl PgMetadataCatalogRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetadataCatalogRepo for PgMetadataCatalogRepo {
    async fn apply_snapshot(
        &self,
        params: &MetadataApplyParams,
    ) -> Result<MetadataApplyResult, BackendError> {
        reflect::apply(self, params).await
    }
    async fn verified_binding(
        &self,
        tenant: &str,
        object_id: &str,
        object_version: i64,
    ) -> Result<Option<MetadataEngineBinding>, BackendError> {
        storage::verified(&self.pool, tenant, object_id, object_version).await
    }
}
