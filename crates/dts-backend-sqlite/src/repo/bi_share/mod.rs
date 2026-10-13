// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use dts_backend::{BackendError, repo::bi_share::*};
use sqlx::{Sqlite, SqlitePool, Transaction};
mod storage;
mod writes;

pub struct SqliteBiShareRepo {
    pool: SqlitePool,
}
impl SqliteBiShareRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    async fn begin(&self) -> Result<Transaction<'_, Sqlite>, BackendError> {
        self.pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage::error)
    }
}
#[async_trait]
impl BiShareRepo for SqliteBiShareRepo {
    async fn create(&self, params: &CreateBiShareParams) -> Result<BiShare, BackendError> {
        validation::create(params)?;
        let mut tx = self.begin().await?;
        let value = writes::create(&mut tx, params).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn get(&self, lookup: &BiShareLookup) -> Result<Option<BiShare>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        storage::authority(&mut tx, &lookup.tenant_id, &lookup.actor_id, false).await?;
        let value = storage::get(&mut tx, lookup).await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn list(&self, filter: &BiShareFilter) -> Result<BiSharePage, BackendError> {
        validation::filter(filter)?;
        let mut tx = self.pool.begin().await.map_err(storage::error)?;
        storage::authority(&mut tx, &filter.tenant_id, &filter.actor_id, false).await?;
        let total=sqlx::query_scalar("SELECT COUNT(*) FROM bi_share WHERE tenant_id=?1 AND (owner_id=?2 OR recipient_id=?2) AND (?3 IS NULL OR report_id=?3)")
            .bind(&filter.tenant_id).bind(&filter.actor_id).bind(&filter.report_id).fetch_one(&mut *tx).await.map_err(storage::error)?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM bi_share WHERE tenant_id=?1 AND (owner_id=?2 OR recipient_id=?2) AND (?3 IS NULL OR report_id=?3) ORDER BY created_at DESC,id LIMIT ?4 OFFSET ?5")
            .bind(&filter.tenant_id).bind(&filter.actor_id).bind(&filter.report_id).bind(i64::from(filter.limit)).bind(i64::from(filter.offset)).fetch_all(&mut *tx).await.map_err(storage::error)?;
        let mut data = Vec::with_capacity(ids.len());
        for id in ids {
            data.push(
                storage::required(
                    &mut tx,
                    &BiShareLookup {
                        tenant_id: filter.tenant_id.clone(),
                        actor_id: filter.actor_id.clone(),
                        share_id: id,
                    },
                )
                .await?,
            );
        }
        tx.commit().await.map_err(storage::error)?;
        Ok(BiSharePage { data, total })
    }
    async fn open(&self, lookup: &BiShareLookup) -> Result<BiShare, BackendError> {
        let mut tx = self.begin().await?;
        storage::authority(&mut tx, &lookup.tenant_id, &lookup.actor_id, false).await?;
        let value = storage::required(&mut tx, lookup).await?;
        if value.status != BiShareStatus::Active || value.is_expired {
            return Err(storage::invalid("分享已经撤销或到期"));
        }
        storage::authority(&mut tx, &value.tenant_id, &value.owner_id, true).await?;
        storage::authority(&mut tx, &value.tenant_id, &value.recipient_id, false).await?;
        writes::binding(
            &mut tx,
            &value.tenant_id,
            &value.report_id,
            value.report_version,
            &value.job_id,
            &value.node_id,
        )
        .await?;
        storage::audit(&mut tx, &value, &lookup.actor_id, "open").await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
    async fn revoke(&self, params: &RevokeBiShareParams) -> Result<BiShare, BackendError> {
        let mut tx = self.begin().await?;
        storage::authority(&mut tx, &params.tenant_id, &params.actor_id, true).await?;
        let lookup = BiShareLookup {
            tenant_id: params.tenant_id.clone(),
            actor_id: params.actor_id.clone(),
            share_id: params.share_id.clone(),
        };
        let original = storage::required(&mut tx, &lookup).await?;
        if original.owner_id != params.actor_id
            || original.version != params.expected_version
            || original.status != BiShareStatus::Active
        {
            return Err(storage::invalid("仅真实分享者可按当前版本撤销有效分享"));
        }
        let updated=sqlx::query("UPDATE bi_share SET status='revoked',version=version+1,revoked_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE tenant_id=?1 AND id=?2 AND owner_id=?3 AND version=?4 AND status='active'")
            .bind(&params.tenant_id).bind(&params.share_id).bind(&params.actor_id).bind(params.expected_version).execute(&mut *tx).await.map_err(storage::error)?;
        if updated.rows_affected() != 1 {
            return Err(storage::invalid("分享撤销并发冲突"));
        }
        let value = storage::required(&mut tx, &lookup).await?;
        storage::audit(&mut tx, &value, &params.actor_id, "revoke").await?;
        tx.commit().await.map_err(storage::error)?;
        Ok(value)
    }
}
