// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::notification_connection::*};
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};

use super::product_resource::map_reference_error as map_error;

pub struct PgNotificationConnectionRepo {
    pool: PgPool,
}

impl PgNotificationConnectionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn decode(row: &sqlx::postgres::PgRow) -> Result<NotificationConnection, BackendError> {
        let secret: Option<String> = row.try_get("encrypted_secret").map_err(map_error)?;
        let has_secret = row
            .try_get::<bool, _>("has_secret")
            .unwrap_or_else(|_| secret.is_some());
        Ok(NotificationConnection {
            id: row.try_get("id").map_err(map_error)?,
            tenant_id: row.try_get("tenant_id").map_err(map_error)?,
            name: row.try_get("name").map_err(map_error)?,
            channel: row.try_get("channel").map_err(map_error)?,
            public_config: row.try_get("public_config").map_err(map_error)?,
            secret,
            has_secret,
            enabled: row.try_get("enabled").map_err(map_error)?,
            version: row.try_get("version").map_err(map_error)?,
            last_test_status: row.try_get("last_test_status").map_err(map_error)?,
            last_test_error: row.try_get("last_test_error").map_err(map_error)?,
            last_test_at: row.try_get("last_test_at").map_err(map_error)?,
            created_by: row.try_get("created_by").map_err(map_error)?,
            updated_by: row.try_get("updated_by").map_err(map_error)?,
            created_at: row.try_get("created_at").map_err(map_error)?,
            updated_at: row.try_get("updated_at").map_err(map_error)?,
        })
    }

    async fn read(
        tx: &mut Transaction<'_, Postgres>,
        tenant: &str,
        id: &str,
    ) -> Result<Option<NotificationConnection>, BackendError> {
        let row = sqlx::query(
            "SELECT * FROM notification_connection
             WHERE tenant_id=$1 AND id=$2 AND deleted_at IS NULL",
        )
        .bind(tenant)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(map_error)?;
        row.as_ref().map(Self::decode).transpose()
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        tx: &mut Transaction<'_, Postgres>,
        tenant: &str,
        id: &str,
        actor: &str,
        operation: &str,
        version: i64,
        summary: serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<(), BackendError> {
        sqlx::query(
            "INSERT INTO notification_connection_audit
             (id,tenant_id,connection_id,actor_id,operation,version,summary,created_at)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(tenant)
        .bind(id)
        .bind(actor)
        .bind(operation)
        .bind(version)
        .bind(summary)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
        Ok(())
    }

    async fn stale_or_missing(&self, tenant: &str, id: &str) -> BackendError {
        match sqlx::query_scalar::<_, i64>(
            "SELECT version FROM notification_connection
             WHERE tenant_id=$1 AND id=$2 AND deleted_at IS NULL",
        )
        .bind(tenant)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(Some(_)) => BackendError::Conflict("通知连接版本已经变化，请刷新后重试".into()),
            _ => BackendError::NotFound("通知连接不存在或不属于当前租户".into()),
        }
    }
}

#[async_trait]
impl NotificationConnectionRepo for PgNotificationConnectionRepo {
    async fn create(
        &self,
        p: &CreateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO notification_connection
             (id,tenant_id,name,channel,public_config,encrypted_secret,enabled,version,created_by,updated_by,created_at,updated_at)
             VALUES($1,$2,$3,$4,$5,$6,$7,1,$8,$8,$9,$9)",
        )
        .bind(&p.id)
        .bind(&p.tenant_id)
        .bind(&p.name)
        .bind(&p.channel)
        .bind(&p.public_config)
        .bind(&p.secret)
        .bind(p.enabled)
        .bind(&p.actor_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        Self::audit(
            &mut tx,
            &p.tenant_id,
            &p.id,
            &p.actor_id,
            "create",
            1,
            json!({"name":p.name,"channel":p.channel,"enabled":p.enabled}),
            now,
        )
        .await?;
        let value = Self::read(&mut tx, &p.tenant_id, &p.id)
            .await?
            .ok_or_else(|| BackendError::Internal("创建通知连接后无法读取".into()))?;
        tx.commit().await.map_err(map_error)?;
        Ok(value)
    }

    async fn list(&self, tenant: &str) -> Result<Vec<NotificationConnection>, BackendError> {
        let rows = sqlx::query(
            "SELECT id,tenant_id,name,channel,public_config,NULL::TEXT AS encrypted_secret,
                    encrypted_secret IS NOT NULL AS has_secret,enabled,version,last_test_status,last_test_error,last_test_at,
                    created_by,updated_by,created_at,updated_at
             FROM notification_connection
             WHERE tenant_id=$1 AND deleted_at IS NULL ORDER BY name,id",
        )
        .bind(tenant)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?;
        rows.iter().map(Self::decode).collect()
    }

    async fn get(
        &self,
        tenant: &str,
        id: &str,
    ) -> Result<Option<NotificationConnection>, BackendError> {
        let row = sqlx::query(
            "SELECT * FROM notification_connection
             WHERE tenant_id=$1 AND id=$2 AND deleted_at IS NULL",
        )
        .bind(tenant)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_error)?;
        row.as_ref().map(Self::decode).transpose()
    }

    async fn update(
        &self,
        p: &UpdateNotificationConnectionParams,
    ) -> Result<NotificationConnection, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE notification_connection
             SET name=$1,public_config=$2,
                 encrypted_secret=CASE WHEN $3 THEN $4 ELSE encrypted_secret END,
                 version=version+1,updated_by=$5,updated_at=$6
             WHERE tenant_id=$7 AND id=$8 AND version=$9 AND deleted_at IS NULL",
        )
        .bind(&p.name)
        .bind(&p.public_config)
        .bind(p.replace_secret)
        .bind(&p.secret)
        .bind(&p.actor_id)
        .bind(now)
        .bind(&p.tenant_id)
        .bind(&p.id)
        .bind(p.expected_version)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() != 1 {
            drop(tx);
            return Err(self.stale_or_missing(&p.tenant_id, &p.id).await);
        }
        let version = p.expected_version + 1;
        Self::audit(
            &mut tx,
            &p.tenant_id,
            &p.id,
            &p.actor_id,
            "update",
            version,
            json!({"name":p.name,"secret_replaced":p.replace_secret}),
            now,
        )
        .await?;
        let value = Self::read(&mut tx, &p.tenant_id, &p.id)
            .await?
            .ok_or_else(|| BackendError::Internal("更新通知连接后无法读取".into()))?;
        tx.commit().await.map_err(map_error)?;
        Ok(value)
    }

    async fn set_enabled(
        &self,
        p: &SetNotificationConnectionEnabledParams,
    ) -> Result<NotificationConnection, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE notification_connection
             SET enabled=$1,version=version+1,updated_by=$2,updated_at=$3
             WHERE tenant_id=$4 AND id=$5 AND version=$6 AND deleted_at IS NULL",
        )
        .bind(p.enabled)
        .bind(&p.actor_id)
        .bind(now)
        .bind(&p.tenant_id)
        .bind(&p.id)
        .bind(p.expected_version)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() != 1 {
            drop(tx);
            return Err(self.stale_or_missing(&p.tenant_id, &p.id).await);
        }
        let version = p.expected_version + 1;
        Self::audit(
            &mut tx,
            &p.tenant_id,
            &p.id,
            &p.actor_id,
            if p.enabled { "enable" } else { "disable" },
            version,
            json!({"enabled":p.enabled}),
            now,
        )
        .await?;
        let value = Self::read(&mut tx, &p.tenant_id, &p.id)
            .await?
            .ok_or_else(|| BackendError::Internal("更新通知连接后无法读取".into()))?;
        tx.commit().await.map_err(map_error)?;
        Ok(value)
    }

    async fn delete(
        &self,
        tenant: &str,
        id: &str,
        expected: i64,
        actor: &str,
    ) -> Result<bool, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE notification_connection
             SET enabled=FALSE,version=version+1,updated_by=$1,updated_at=$2,deleted_at=$2
             WHERE tenant_id=$3 AND id=$4 AND version=$5 AND deleted_at IS NULL",
        )
        .bind(actor)
        .bind(now)
        .bind(tenant)
        .bind(id)
        .bind(expected)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() != 1 {
            drop(tx);
            return Err(self.stale_or_missing(tenant, id).await);
        }
        Self::audit(
            &mut tx,
            tenant,
            id,
            actor,
            "delete",
            expected + 1,
            json!({}),
            now,
        )
        .await?;
        tx.commit().await.map_err(map_error)?;
        Ok(true)
    }

    async fn record_test(
        &self,
        p: &RecordNotificationConnectionTestParams,
    ) -> Result<(), BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let result = sqlx::query(
            "UPDATE notification_connection
             SET last_test_status=$1,last_test_error=$2,last_test_at=$3
             WHERE tenant_id=$4 AND id=$5 AND deleted_at IS NULL",
        )
        .bind(&p.status)
        .bind(&p.error)
        .bind(p.tested_at)
        .bind(&p.tenant_id)
        .bind(&p.id)
        .execute(&mut *tx)
        .await
        .map_err(map_error)?;
        if result.rows_affected() != 1 {
            return Err(BackendError::NotFound(
                "通知连接不存在或不属于当前租户".into(),
            ));
        }
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM notification_connection WHERE tenant_id=$1 AND id=$2",
        )
        .bind(&p.tenant_id)
        .bind(&p.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_error)?;
        Self::audit(
            &mut tx,
            &p.tenant_id,
            &p.id,
            &p.actor_id,
            "test",
            version,
            json!({"status":p.status,"error":p.error}),
            p.tested_at,
        )
        .await?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }
}
