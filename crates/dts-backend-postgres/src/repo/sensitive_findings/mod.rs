// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use dts_backend::{BackendError, repo::sensitive_findings::*};
use sqlx::PgPool;
mod producer;
mod review;
mod storage;
pub struct PgSensitiveFindingRepo {
    pool: PgPool,
}
impl PgSensitiveFindingRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl SensitiveFindingRepo for PgSensitiveFindingRepo {
    async fn commit_scan(
        &self,
        params: &SensitiveCommitParams,
    ) -> Result<SensitiveScanResult, BackendError> {
        producer::commit(self, params).await
    }
    async fn review(
        &self,
        params: &SensitiveReviewParams,
    ) -> Result<SensitiveFinding, BackendError> {
        review::decide(self, params).await
    }
    async fn get(
        &self,
        tenant: &str,
        actor: &str,
        id: &str,
    ) -> Result<Option<SensitiveFinding>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        storage::authority(&mut tx, tenant, actor, "read").await?;
        let value = storage::get(&mut tx, tenant, id).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list(
        &self,
        filter: &SensitiveFindingFilter,
    ) -> Result<SensitiveFindingPage, BackendError> {
        if !(1..=100).contains(&filter.limit)
            || filter
                .status
                .as_ref()
                .is_some_and(|v| !matches!(v.as_str(), "pending" | "confirmed" | "dismissed"))
        {
            return Err(storage::invalid("发现列表筛选或分页无效"));
        }
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        storage::authority(&mut tx, &filter.tenant_id, &filter.actor_id, "read").await?;
        let total=sqlx::query_scalar("SELECT COUNT(*) FROM sensitive_finding WHERE tenant_id=$1 AND ($2 IS NULL OR task_id=$2) AND ($3 IS NULL OR status=$3)").bind(&filter.tenant_id).bind(&filter.task_id).bind(&filter.status).fetch_one(&mut *tx).await.map_err(storage::error)?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM sensitive_finding WHERE tenant_id=$1 AND ($2 IS NULL OR task_id=$2) AND ($3 IS NULL OR status=$3) ORDER BY created_at DESC,id LIMIT $4 OFFSET $5").bind(&filter.tenant_id).bind(&filter.task_id).bind(&filter.status).bind(i64::from(filter.limit)).bind(i64::from(filter.offset)).fetch_all(&mut *tx).await.map_err(storage::error)?;
        let mut data = Vec::with_capacity(ids.len());
        for id in ids {
            data.push(storage::required(&mut tx, &filter.tenant_id, &id).await?);
        }
        tx.commit().await.map_err(storage::error)?;
        Ok(SensitiveFindingPage { data, total })
    }
}
