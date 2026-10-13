// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use dts_backend::{BackendError, repo::product_job::*};
use sqlx::{PgPool, Postgres, Transaction};

mod control;
mod execution;
mod read;
mod writes;

pub struct PgProductJobRepo {
    pool: PgPool,
}

impl PgProductJobRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, BackendError> {
        self.pool.begin().await.map_err(read::error)
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(read::error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(read::error)?;
        Ok(tx)
    }
}

#[async_trait]
impl ProductJobRepo for PgProductJobRepo {
    async fn enqueue(
        &self,
        params: &EnqueueProductJobParams,
    ) -> Result<EnqueuedProductJob, BackendError> {
        writes::enqueue(self, params).await
    }

    async fn list(&self, filter: &ProductJobFilter) -> Result<ProductJobPage, BackendError> {
        let status = filter
            .status
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| BackendError::Internal(e.to_string()))?;
        let status = status.as_ref().and_then(|value| value.as_str());
        let mut tx = self.begin_read().await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_job WHERE tenant_id = $1
            AND ($2 IS NULL OR actor_id = $2) AND ($3 IS NULL OR source_kind = $3)
            AND ($4 IS NULL OR source_id = $4) AND ($5 IS NULL OR status = $5)",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.actor_id)
        .bind(&filter.source_kind)
        .bind(&filter.source_id)
        .bind(status)
        .fetch_one(&mut *tx)
        .await
        .map_err(read::error)?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM product_job WHERE tenant_id = $1
            AND ($2 IS NULL OR actor_id = $2) AND ($3 IS NULL OR source_kind = $3)
            AND ($4 IS NULL OR source_id = $4) AND ($5 IS NULL OR status = $5)
            ORDER BY created_at DESC, id LIMIT $6 OFFSET $7",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.actor_id)
        .bind(&filter.source_kind)
        .bind(&filter.source_id)
        .bind(status)
        .bind(i64::from(filter.limit.clamp(1, 100)))
        .bind(i64::from(filter.offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(read::error)?;
        let mut data = Vec::with_capacity(ids.len());
        for id in ids {
            data.push(read::required(&mut tx, &filter.tenant_id, &id).await?);
        }
        tx.commit().await.map_err(read::error)?;
        Ok(ProductJobPage { data, total })
    }

    async fn get(&self, lookup: &ProductJobLookup) -> Result<Option<ProductJob>, BackendError> {
        let mut tx = self.begin_read().await?;
        let job = read::get(
            &mut tx,
            &lookup.tenant_id,
            &lookup.job_id,
            lookup.actor_id.as_deref(),
        )
        .await?;
        tx.commit().await.map_err(read::error)?;
        Ok(job)
    }

    async fn claim(
        &self,
        params: &ClaimProductJobParams,
    ) -> Result<Option<ProductJob>, BackendError> {
        writes::claim(self, params).await
    }
    async fn heartbeat(
        &self,
        params: &HeartbeatProductJobParams,
    ) -> Result<ProductJob, BackendError> {
        writes::heartbeat(self, params).await
    }
    async fn start_node(
        &self,
        params: &StartProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError> {
        execution::start_node(self, params).await
    }
    async fn finish_node(
        &self,
        params: &FinishProductJobNodeParams,
    ) -> Result<ProductJobNode, BackendError> {
        execution::finish_node(self, params).await
    }
    async fn finish(&self, params: &FinishProductJobParams) -> Result<ProductJob, BackendError> {
        execution::finish(self, params).await
    }
    async fn request_cancel(
        &self,
        params: &CancelProductJobParams,
    ) -> Result<ProductJob, BackendError> {
        control::cancel(self, params).await
    }
    async fn reconcile(
        &self,
        params: &ReconcileProductJobParams,
    ) -> Result<ProductJob, BackendError> {
        control::reconcile(self, params).await
    }
}
