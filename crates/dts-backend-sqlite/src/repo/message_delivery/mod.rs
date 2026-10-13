// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::Utc;
use dts_backend::{BackendError, repo::message_delivery::*};
use sqlx::{Row, SqlitePool};

mod helpers;
use helpers::{audit_read, audit_send, decode, map_error, validate_source};

pub struct SqliteMessageDeliveryRepo {
    pool: SqlitePool,
}

impl SqliteMessageDeliveryRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageDeliveryRepo for SqliteMessageDeliveryRepo {
    async fn get_batch(
        &self,
        tenant_id: &str,
        message_id: &str,
        expected_version: i64,
        actor_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<MessageDeliveryBatch>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let batch=sqlx::query("SELECT id,message_id,message_version,actor_id,resource_version FROM message_delivery_batch WHERE tenant_id=?1 AND idempotency_key=?2")
            .bind(tenant_id).bind(idempotency_key).fetch_optional(&mut *tx).await.map_err(map_error)?;
        let Some(batch) = batch else {
            tx.commit().await.map_err(map_error)?;
            return Ok(None);
        };
        if batch
            .try_get::<String, _>("message_id")
            .map_err(map_error)?
            != message_id
            || batch.try_get::<String, _>("actor_id").map_err(map_error)? != actor_id
            || batch
                .try_get::<i64, _>("message_version")
                .map_err(map_error)?
                != expected_version
        {
            return Err(BackendError::Conflict(
                "幂等标识已经用于另一条投递请求".into(),
            ));
        }
        let id: String = batch.try_get("id").map_err(map_error)?;
        let rows=sqlx::query("SELECT * FROM message_delivery WHERE tenant_id=?1 AND batch_id=?2 ORDER BY recipient_id")
            .bind(tenant_id).bind(id).fetch_all(&mut *tx).await.map_err(map_error)?;
        let data = rows.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
        let resource_version = batch.try_get("resource_version").map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(Some(MessageDeliveryBatch {
            data,
            resource_version,
            replayed: true,
        }))
    }
    async fn deliver_in_app(
        &self,
        params: &DeliverInAppParams,
    ) -> Result<MessageDeliveryBatch, BackendError> {
        let request = delivery_request(params)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_error)?;
        let batch = sqlx::query("SELECT id, request, resource_version FROM message_delivery_batch WHERE tenant_id=?1 AND idempotency_key=?2")
            .bind(&params.tenant_id).bind(&params.idempotency_key).fetch_optional(&mut *tx).await.map_err(map_error)?;
        if let Some(batch) = batch {
            let stored: String = batch.try_get("request").map_err(map_error)?;
            let stored: serde_json::Value = serde_json::from_str(&stored)
                .map_err(|_| BackendError::Internal("持久化消息批次格式无效".into()))?;
            if stored != request {
                return Err(BackendError::Conflict(
                    "幂等标识已经用于另一条投递请求".into(),
                ));
            }
            let id: String = batch.try_get("id").map_err(map_error)?;
            let rows = sqlx::query("SELECT * FROM message_delivery WHERE tenant_id=?1 AND batch_id=?2 ORDER BY recipient_id")
                .bind(&params.tenant_id).bind(&id).fetch_all(&mut *tx).await.map_err(map_error)?;
            let data = rows.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
            let resource_version = batch.try_get("resource_version").map_err(map_error)?;
            tx.commit().await.map_err(map_error)?;
            return Ok(MessageDeliveryBatch {
                data,
                resource_version,
                replayed: true,
            });
        }
        validate_source(&mut tx, params, &request).await?;
        let batch_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO message_delivery_batch (id,tenant_id,message_id,message_version,resource_version,actor_id,idempotency_key,request,delivered_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")
            .bind(&batch_id).bind(&params.tenant_id).bind(&params.message_id).bind(params.expected_version)
            .bind(params.expected_version+1).bind(&params.actor_id).bind(&params.idempotency_key)
            .bind(request.to_string()).bind(&now).execute(&mut *tx).await.map_err(map_error)?;
        let mut data = Vec::with_capacity(params.recipient_ids.len());
        for recipient in &params.recipient_ids {
            let id = uuid::Uuid::new_v4().to_string();
            let row = sqlx::query("INSERT INTO message_delivery (id,tenant_id,batch_id,message_id,message_version,recipient_id,subject,body,delivered_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) RETURNING *")
                .bind(&id).bind(&params.tenant_id).bind(&batch_id).bind(&params.message_id).bind(params.expected_version)
                .bind(recipient).bind(&params.subject).bind(&params.body).bind(&now)
                .fetch_one(&mut *tx).await.map_err(map_error)?;
            data.push(decode(&row)?);
        }
        let changed = sqlx::query("UPDATE product_resource SET status='sent',version=version+1,updated_at=?1 WHERE tenant_id=?2 AND kind='system_message' AND id=?3 AND version=?4 AND status='draft'")
            .bind(&now).bind(&params.tenant_id).bind(&params.message_id).bind(params.expected_version)
            .execute(&mut *tx).await.map_err(map_error)?;
        if changed.rows_affected() != 1 {
            return Err(BackendError::Conflict("消息状态或版本已经变化".into()));
        }
        audit_send(&mut tx, params, &now).await?;
        tx.commit().await.map_err(map_error)?;
        Ok(MessageDeliveryBatch {
            data,
            resource_version: params.expected_version + 1,
            replayed: false,
        })
    }

    async fn list_inbox(
        &self,
        filter: &MessageInboxFilter,
    ) -> Result<MessageInboxPage, BackendError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let total = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT id,read_at FROM message_delivery WHERE tenant_id=?1 AND recipient_id=?2 UNION ALL SELECT d.id,r.read_at FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id LEFT JOIN alert_policy_delivery_read r ON r.tenant_id=d.tenant_id AND r.delivery_id=d.id WHERE d.tenant_id=?1 AND d.recipient_id=?2 AND d.status='delivered') inbox WHERE (?3=0 OR read_at IS NULL)")
            .bind(&filter.tenant_id).bind(&filter.recipient_id).bind(filter.unread_only)
            .fetch_one(&mut *tx).await.map_err(map_error)?;
        let rows = sqlx::query("SELECT * FROM (SELECT id,tenant_id,source_kind,message_id,message_version,recipient_id,subject,body,delivered_at,read_at FROM message_delivery WHERE tenant_id=?1 AND recipient_id=?2 UNION ALL SELECT d.id,d.tenant_id,'alert_policy' AS source_kind,e.policy_id AS message_id,e.policy_version AS message_version,d.recipient_id,d.subject,d.body,s.delivered_at,r.read_at FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id JOIN alert_policy_evaluation e ON e.tenant_id=d.tenant_id AND e.id=d.evaluation_id LEFT JOIN alert_policy_delivery_read r ON r.tenant_id=d.tenant_id AND r.delivery_id=d.id WHERE d.tenant_id=?1 AND d.recipient_id=?2 AND d.status='delivered') inbox WHERE (?3=0 OR read_at IS NULL) ORDER BY delivered_at DESC,id DESC LIMIT ?4 OFFSET ?5")
            .bind(&filter.tenant_id).bind(&filter.recipient_id).bind(filter.unread_only)
            .bind(i64::from(filter.limit.clamp(1,100))).bind(i64::from(filter.offset))
            .fetch_all(&mut *tx).await.map_err(map_error)?;
        let data = rows.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await.map_err(map_error)?;
        Ok(MessageInboxPage { data, total })
    }

    async fn mark_read(
        &self,
        tenant_id: &str,
        recipient_id: &str,
        delivery_id: &str,
    ) -> Result<MessageDelivery, BackendError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(map_error)?;
        let row = sqlx::query(
            "SELECT * FROM message_delivery WHERE tenant_id=?1 AND recipient_id=?2 AND id=?3",
        )
        .bind(tenant_id)
        .bind(recipient_id)
        .bind(delivery_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_error)?;
        if row.is_none() {
            let row=sqlx::query("SELECT d.id,d.tenant_id,'alert_policy' AS source_kind,e.policy_id AS message_id,e.policy_version AS message_version,d.recipient_id,d.subject,d.body,s.delivered_at,r.read_at FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id JOIN alert_policy_evaluation e ON e.tenant_id=d.tenant_id AND e.id=d.evaluation_id LEFT JOIN alert_policy_delivery_read r ON r.tenant_id=d.tenant_id AND r.delivery_id=d.id WHERE d.tenant_id=?1 AND d.recipient_id=?2 AND d.id=?3 AND d.status='delivered'")
                .bind(tenant_id).bind(recipient_id).bind(delivery_id).fetch_optional(&mut *tx).await.map_err(map_error)?
                .ok_or_else(|| BackendError::NotFound("收件记录不存在".into()))?;
            let before = decode(&row)?;
            if before.read_at.is_some() {
                tx.commit().await.map_err(map_error)?;
                return Ok(before);
            }
            let now = Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO alert_policy_delivery_read(tenant_id,delivery_id,read_at) VALUES(?1,?2,?3)").bind(tenant_id).bind(delivery_id).bind(&now).execute(&mut *tx).await.map_err(map_error)?;
            audit_read(&mut tx, &before, &now).await?;
            let row=sqlx::query("SELECT d.id,d.tenant_id,'alert_policy' AS source_kind,e.policy_id AS message_id,e.policy_version AS message_version,d.recipient_id,d.subject,d.body,s.delivered_at,r.read_at FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id JOIN alert_policy_evaluation e ON e.tenant_id=d.tenant_id AND e.id=d.evaluation_id JOIN alert_policy_delivery_read r ON r.tenant_id=d.tenant_id AND r.delivery_id=d.id WHERE d.tenant_id=?1 AND d.recipient_id=?2 AND d.id=?3").bind(tenant_id).bind(recipient_id).bind(delivery_id).fetch_one(&mut *tx).await.map_err(map_error)?;
            let result = decode(&row)?;
            tx.commit().await.map_err(map_error)?;
            return Ok(result);
        }
        let Some(row) = row else {
            return Err(BackendError::Internal("收件记录分支状态无效".into()));
        };
        let before = decode(&row)?;
        if before.read_at.is_some() {
            tx.commit().await.map_err(map_error)?;
            return Ok(before);
        }
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query("UPDATE message_delivery SET read_at=?1 WHERE tenant_id=?2 AND recipient_id=?3 AND id=?4 AND read_at IS NULL RETURNING *")
            .bind(&now).bind(tenant_id).bind(recipient_id).bind(delivery_id)
            .fetch_one(&mut *tx).await.map_err(map_error)?;
        audit_read(&mut tx, &before, &now).await?;
        let record = decode(&row)?;
        tx.commit().await.map_err(map_error)?;
        Ok(record)
    }
}
