// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dts_backend::{BackendError, repo::alert_policy::*};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::product_resource::map_reference_error as error;

pub struct SqliteAlertPolicyRepo {
    pool: SqlitePool,
}

impl SqliteAlertPolicyRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn time(value: String) -> Result<DateTime<Utc>, BackendError> {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|e| BackendError::Internal(format!("告警时间无效: {e}")))
    }

    async fn deliveries(
        conn: &mut SqliteConnection,
        tenant: &str,
        evaluation: &str,
    ) -> Result<Vec<AlertPolicyDelivery>, BackendError> {
        let rows = sqlx::query("SELECT d.id,d.recipient_id,d.subject,d.body,d.status,d.attempt_count,d.last_error,d.created_at,COALESCE(s.channel,'in_app') AS channel,s.next_attempt_at,s.delivered_at FROM alert_policy_delivery d LEFT JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id WHERE d.tenant_id=?1 AND d.evaluation_id=?2 ORDER BY d.recipient_id")
            .bind(tenant).bind(evaluation).fetch_all(conn).await.map_err(error)?;
        rows.iter().map(Self::decode_delivery).collect()
    }

    fn optional_time(value: Option<String>) -> Result<Option<DateTime<Utc>>, BackendError> {
        value.map(Self::time).transpose()
    }

    fn decode_delivery(row: &sqlx::sqlite::SqliteRow) -> Result<AlertPolicyDelivery, BackendError> {
        Ok(AlertPolicyDelivery {
            id: row.try_get("id").map_err(error)?,
            recipient_id: row.try_get("recipient_id").map_err(error)?,
            subject: row.try_get("subject").map_err(error)?,
            body: row.try_get("body").map_err(error)?,
            channel: row.try_get("channel").map_err(error)?,
            status: row.try_get("status").map_err(error)?,
            attempt_count: row.try_get("attempt_count").map_err(error)?,
            last_error: row.try_get("last_error").map_err(error)?,
            next_attempt_at: Self::optional_time(row.try_get("next_attempt_at").map_err(error)?)?,
            delivered_at: Self::optional_time(row.try_get("delivered_at").map_err(error)?)?,
            created_at: Self::time(row.try_get("created_at").map_err(error)?)?,
        })
    }

    async fn decode(
        conn: &mut SqliteConnection,
        row: &sqlx::sqlite::SqliteRow,
        replayed: bool,
    ) -> Result<AlertPolicyEvaluation, BackendError> {
        let tenant_id: String = row.try_get("tenant_id").map_err(error)?;
        let id: String = row.try_get("id").map_err(error)?;
        Ok(AlertPolicyEvaluation {
            deliveries: Self::deliveries(conn, &tenant_id, &id).await?,
            id,
            tenant_id,
            policy_id: row.try_get("policy_id").map_err(error)?,
            policy_version: row.try_get("policy_version").map_err(error)?,
            event_type: row.try_get("event_type").map_err(error)?,
            window_start: Self::time(row.try_get("window_start").map_err(error)?)?,
            window_end: Self::time(row.try_get("window_end").map_err(error)?)?,
            observed_count: row.try_get("observed_count").map_err(error)?,
            threshold: row.try_get("threshold").map_err(error)?,
            state: row.try_get("state").map_err(error)?,
            cooldown_until: row
                .try_get::<Option<String>, _>("cooldown_until")
                .map_err(error)?
                .map(Self::time)
                .transpose()?,
            actor_id: row.try_get("actor_id").map_err(error)?,
            created_at: Self::time(row.try_get("created_at").map_err(error)?)?,
            replayed,
        })
    }

    async fn observed(
        conn: &mut SqliteConnection,
        tenant: &str,
        event_type: &str,
        start: DateTime<Utc>,
    ) -> Result<i64, BackendError> {
        let start = start.to_rfc3339();
        let value = match event_type {
            "task_failure" => sqlx::query_scalar("SELECT COUNT(*) FROM task_run r JOIN sync_task t ON t.id=r.task_id WHERE t.tenant_id=?1 AND r.status='failed' AND julianday(COALESCE(r.end_time,r.created_at))>=julianday(?2)")
                .bind(tenant).bind(&start).fetch_one(&mut *conn).await,
            "quality_failure" => sqlx::query_scalar("SELECT COUNT(*) FROM quality_observation WHERE tenant_id=?1 AND state='threshold_exceeded' AND julianday(created_at)>=julianday(?2)")
                .bind(tenant).bind(&start).fetch_one(&mut *conn).await,
            "system_health" => sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE tenant_id=?1 AND health_status NOT IN('healthy','unknown') AND last_test_at IS NOT NULL AND julianday(last_test_at)>=julianday(?2)")
                .bind(tenant).bind(&start).fetch_one(&mut *conn).await,
            _ => return Err(BackendError::ConstraintViolation("未知告警事件类型".into())),
        };
        value.map_err(error)
    }
}

#[async_trait]
impl AlertPolicyRepo for SqliteAlertPolicyRepo {
    async fn evaluate(
        &self,
        params: &EvaluateAlertPolicyParams,
    ) -> Result<AlertPolicyEvaluation, BackendError> {
        let hash = evaluation_hash(params)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        if let Some(row) = sqlx::query(
            "SELECT * FROM alert_policy_evaluation WHERE tenant_id=?1 AND idempotency_key=?2",
        )
        .bind(&params.tenant_id)
        .bind(&params.idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(error)?
        {
            if row.try_get::<String, _>("request_hash").map_err(error)? != hash {
                return Err(BackendError::Conflict(
                    "幂等键已用于另一告警策略请求".into(),
                ));
            }
            let result = Self::decode(&mut tx, &row, true).await?;
            tx.commit().await.map_err(error)?;
            return Ok(result);
        }
        let version:Option<i64>=sqlx::query_scalar("SELECT version FROM product_resource WHERE tenant_id=?1 AND kind='alert_policy' AND id=?2")
            .bind(&params.tenant_id).bind(&params.policy_id).fetch_optional(&mut *tx).await.map_err(error)?;
        if version != Some(params.policy_version) {
            return Err(BackendError::Conflict(
                "告警策略不存在或版本已经变化".into(),
            ));
        }
        for recipient in &params.recipient_ids {
            let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" WHERE tenant_id=?1 AND id=?2 AND status='active')")
                .bind(&params.tenant_id).bind(recipient).fetch_one(&mut *tx).await.map_err(error)?;
            if !active {
                return Err(BackendError::Forbidden(
                    "告警收件人不存在、停用或不属于当前租户".into(),
                ));
            }
        }
        let now = Utc::now();
        let window_start = now - Duration::seconds(params.window_seconds);
        let observed_count = if params.test_delivery {
            0
        } else {
            Self::observed(&mut tx, &params.tenant_id, &params.event_type, window_start).await?
        };
        let last_trigger:Option<String>=sqlx::query_scalar("SELECT created_at FROM alert_policy_evaluation WHERE tenant_id=?1 AND policy_id=?2 AND state='triggered' ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(&params.tenant_id).bind(&params.policy_id).fetch_optional(&mut *tx).await.map_err(error)?;
        let previous_cooldown = last_trigger
            .map(Self::time)
            .transpose()?
            .map(|time| time + Duration::seconds(params.cooldown_seconds));
        let threshold_met = observed_count >= params.threshold;
        let (state, cooldown_until, deliver) = if params.test_delivery {
            ("test", None, true)
        } else if threshold_met && previous_cooldown.is_some_and(|until| until > now) {
            ("suppressed", previous_cooldown, false)
        } else if threshold_met {
            (
                "triggered",
                Some(now + Duration::seconds(params.cooldown_seconds)),
                true,
            )
        } else {
            ("within_threshold", None, false)
        };
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO alert_policy_evaluation(id,tenant_id,policy_id,policy_version,event_type,window_start,window_end,observed_count,threshold,state,cooldown_until,actor_id,idempotency_key,request_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)")
            .bind(&id).bind(&params.tenant_id).bind(&params.policy_id).bind(params.policy_version).bind(&params.event_type)
            .bind(window_start.to_rfc3339()).bind(now.to_rfc3339()).bind(observed_count).bind(params.threshold).bind(state)
            .bind(cooldown_until.map(|time|time.to_rfc3339())).bind(&params.actor_id).bind(&params.idempotency_key).bind(&hash).bind(now.to_rfc3339())
            .execute(&mut *tx).await.map_err(error)?;
        if deliver {
            for recipient in &params.recipient_ids {
                sqlx::query("INSERT INTO alert_policy_delivery(id,tenant_id,evaluation_id,recipient_id,subject,body,status,attempt_count,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'pending',0,?7,?7)")
                    .bind(uuid::Uuid::new_v4().to_string()).bind(&params.tenant_id).bind(&id).bind(recipient).bind(&params.subject).bind(&params.body).bind(now.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
                let delivery_id:String=sqlx::query_scalar("SELECT id FROM alert_policy_delivery WHERE tenant_id=?1 AND evaluation_id=?2 AND recipient_id=?3")
                    .bind(&params.tenant_id).bind(&id).bind(recipient).fetch_one(&mut *tx).await.map_err(error)?;
                sqlx::query("INSERT INTO alert_policy_delivery_state(tenant_id,delivery_id,channel,next_attempt_at) VALUES(?1,?2,?3,?4)")
                    .bind(&params.tenant_id).bind(delivery_id).bind(&params.channel).bind(now.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
            }
        }
        let summary = serde_json::json!({"action":if params.test_delivery{"test_delivery"}else{"evaluate"},"evaluation_id":id,"state":state,"observed_count":observed_count,"threshold":params.threshold});
        sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,'alert_policy',?4,'update',?5,?6)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&params.tenant_id).bind(&params.actor_id).bind(&params.policy_id).bind(summary.to_string()).bind(now.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
        let row = sqlx::query("SELECT * FROM alert_policy_evaluation WHERE tenant_id=?1 AND id=?2")
            .bind(&params.tenant_id)
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .map_err(error)?;
        let result = Self::decode(&mut tx, &row, false).await?;
        tx.commit().await.map_err(error)?;
        Ok(result)
    }

    async fn list(
        &self,
        filter: &AlertPolicyEvaluationFilter,
    ) -> Result<AlertPolicyEvaluationPage, BackendError> {
        if filter.limit == 0 || filter.limit > 100 {
            return Err(BackendError::ConstraintViolation(
                "告警评估分页必须为 1 至 100".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(error)?;
        let total = sqlx::query_scalar(
            "SELECT COUNT(*) FROM alert_policy_evaluation WHERE tenant_id=?1 AND policy_id=?2",
        )
        .bind(&filter.tenant_id)
        .bind(&filter.policy_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(error)?;
        let rows=sqlx::query("SELECT * FROM alert_policy_evaluation WHERE tenant_id=?1 AND policy_id=?2 ORDER BY created_at DESC,id DESC LIMIT ?3 OFFSET ?4")
            .bind(&filter.tenant_id).bind(&filter.policy_id).bind(i64::from(filter.limit)).bind(i64::from(filter.offset)).fetch_all(&mut *tx).await.map_err(error)?;
        let mut data = Vec::with_capacity(rows.len());
        for row in &rows {
            data.push(Self::decode(&mut tx, row, false).await?);
        }
        tx.commit().await.map_err(error)?;
        Ok(AlertPolicyEvaluationPage { data, total })
    }

    async fn sync_schedules(&self) -> Result<u64, BackendError> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO alert_policy_delivery_state(tenant_id,delivery_id,channel,next_attempt_at) SELECT tenant_id,id,'in_app',created_at FROM alert_policy_delivery WHERE status='pending' ON CONFLICT(tenant_id,delivery_id) DO NOTHING")
            .execute(&mut *tx).await.map_err(error)?;
        let inserted=sqlx::query("INSERT INTO alert_policy_schedule(tenant_id,policy_id,next_evaluation_at,updated_at) SELECT tenant_id,id,?1,?1 FROM product_resource WHERE kind='alert_policy' AND json_extract(properties,'$.enabled')=1 ON CONFLICT(tenant_id,policy_id) DO NOTHING")
            .bind(&now).execute(&mut *tx).await.map_err(error)?.rows_affected();
        sqlx::query("DELETE FROM alert_policy_schedule WHERE NOT EXISTS(SELECT 1 FROM product_resource r WHERE r.tenant_id=alert_policy_schedule.tenant_id AND r.kind='alert_policy' AND r.id=alert_policy_schedule.policy_id AND json_extract(r.properties,'$.enabled')=1)")
            .execute(&mut *tx).await.map_err(error)?;
        tx.commit().await.map_err(error)?;
        Ok(inserted)
    }

    async fn claim_scheduled(
        &self,
        params: &ClaimScheduledAlertPolicyParams,
    ) -> Result<Option<ScheduledAlertPolicy>, BackendError> {
        validate_claim(&params.worker_id, params.lease_seconds, 1)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        let now = Utc::now();
        let row=sqlx::query("SELECT s.tenant_id,s.policy_id,s.next_evaluation_at,r.version,r.created_by FROM alert_policy_schedule s JOIN product_resource r ON r.tenant_id=s.tenant_id AND r.kind='alert_policy' AND r.id=s.policy_id WHERE julianday(s.next_evaluation_at)<=julianday(?1) AND (s.lease_expires_at IS NULL OR julianday(s.lease_expires_at)<=julianday(?1)) AND json_extract(r.properties,'$.enabled')=1 ORDER BY s.next_evaluation_at,s.tenant_id,s.policy_id LIMIT 1")
            .bind(now.to_rfc3339()).fetch_optional(&mut *tx).await.map_err(error)?;
        let Some(row) = row else {
            tx.commit().await.map_err(error)?;
            return Ok(None);
        };
        let tenant_id: String = row.try_get("tenant_id").map_err(error)?;
        let policy_id: String = row.try_get("policy_id").map_err(error)?;
        let scheduled_for = Self::time(row.try_get("next_evaluation_at").map_err(error)?)?;
        let lease_expires_at = now + Duration::seconds(i64::from(params.lease_seconds));
        sqlx::query("UPDATE alert_policy_schedule SET lease_owner=?1,lease_fence=lease_fence+1,lease_expires_at=?2,updated_at=?3 WHERE tenant_id=?4 AND policy_id=?5")
            .bind(&params.worker_id).bind(lease_expires_at.to_rfc3339()).bind(now.to_rfc3339()).bind(&tenant_id).bind(&policy_id).execute(&mut *tx).await.map_err(error)?;
        let lease_fence: i64 = sqlx::query_scalar(
            "SELECT lease_fence FROM alert_policy_schedule WHERE tenant_id=?1 AND policy_id=?2",
        )
        .bind(&tenant_id)
        .bind(&policy_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(error)?;
        tx.commit().await.map_err(error)?;
        Ok(Some(ScheduledAlertPolicy {
            tenant_id,
            policy_id,
            policy_version: row.try_get("version").map_err(error)?,
            actor_id: row.try_get("created_by").map_err(error)?,
            scheduled_for,
            lease_owner: params.worker_id.clone(),
            lease_fence,
        }))
    }

    async fn finish_scheduled(
        &self,
        params: &FinishScheduledAlertPolicyParams,
    ) -> Result<(), BackendError> {
        validate_finish(&params.lease, params.error.as_deref())?;
        let changed=sqlx::query("UPDATE alert_policy_schedule SET next_evaluation_at=?1,lease_owner=NULL,lease_expires_at=NULL,last_error=?2,updated_at=?3 WHERE tenant_id=?4 AND policy_id=?5 AND lease_owner=?6 AND lease_fence=?7")
            .bind(params.next_evaluation_at.to_rfc3339()).bind(&params.error).bind(Utc::now().to_rfc3339()).bind(&params.tenant_id).bind(&params.policy_id).bind(&params.lease.worker_id).bind(params.lease.fence).execute(&self.pool).await.map_err(error)?;
        if changed.rows_affected() != 1 {
            return Err(BackendError::Conflict("告警调度租约已经失效".into()));
        }
        Ok(())
    }

    async fn claim_delivery(
        &self,
        params: &ClaimAlertDeliveryParams,
    ) -> Result<Option<AlertDeliveryClaim>, BackendError> {
        validate_claim(&params.worker_id, params.lease_seconds, params.max_attempts)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        let now = Utc::now();
        sqlx::query("UPDATE alert_policy_delivery SET status='failed',last_error='已达到最大重试次数',updated_at=?1 WHERE status='pending' AND attempt_count>=?2 AND EXISTS(SELECT 1 FROM alert_policy_delivery_state s WHERE s.tenant_id=alert_policy_delivery.tenant_id AND s.delivery_id=alert_policy_delivery.id AND (s.lease_expires_at IS NULL OR julianday(s.lease_expires_at)<=julianday(?1)))")
            .bind(now.to_rfc3339()).bind(i64::from(params.max_attempts)).execute(&mut *tx).await.map_err(error)?;
        let row=sqlx::query("SELECT d.id,d.tenant_id,d.evaluation_id,d.recipient_id,d.subject,d.body,d.attempt_count,s.channel FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id WHERE d.status='pending' AND d.attempt_count<?1 AND julianday(s.next_attempt_at)<=julianday(?2) AND (s.lease_expires_at IS NULL OR julianday(s.lease_expires_at)<=julianday(?2)) ORDER BY s.next_attempt_at,d.created_at,d.id LIMIT 1")
            .bind(i64::from(params.max_attempts)).bind(now.to_rfc3339()).fetch_optional(&mut *tx).await.map_err(error)?;
        let Some(row) = row else {
            tx.commit().await.map_err(error)?;
            return Ok(None);
        };
        let id: String = row.try_get("id").map_err(error)?;
        let tenant_id: String = row.try_get("tenant_id").map_err(error)?;
        sqlx::query("UPDATE alert_policy_delivery SET attempt_count=attempt_count+1,updated_at=?1 WHERE tenant_id=?2 AND id=?3").bind(now.to_rfc3339()).bind(&tenant_id).bind(&id).execute(&mut *tx).await.map_err(error)?;
        sqlx::query("UPDATE alert_policy_delivery_state SET lease_owner=?1,lease_fence=lease_fence+1,lease_expires_at=?2 WHERE tenant_id=?3 AND delivery_id=?4")
            .bind(&params.worker_id).bind((now+Duration::seconds(i64::from(params.lease_seconds))).to_rfc3339()).bind(&tenant_id).bind(&id).execute(&mut *tx).await.map_err(error)?;
        let fence:i64=sqlx::query_scalar("SELECT lease_fence FROM alert_policy_delivery_state WHERE tenant_id=?1 AND delivery_id=?2").bind(&tenant_id).bind(&id).fetch_one(&mut *tx).await.map_err(error)?;
        tx.commit().await.map_err(error)?;
        Ok(Some(AlertDeliveryClaim {
            id,
            tenant_id,
            evaluation_id: row.try_get("evaluation_id").map_err(error)?,
            recipient_id: row.try_get("recipient_id").map_err(error)?,
            subject: row.try_get("subject").map_err(error)?,
            body: row.try_get("body").map_err(error)?,
            channel: row.try_get("channel").map_err(error)?,
            attempt_count: row.try_get::<i64, _>("attempt_count").map_err(error)? + 1,
            lease_owner: params.worker_id.clone(),
            lease_fence: fence,
        }))
    }

    async fn finish_delivery(
        &self,
        params: &FinishAlertDeliveryParams,
    ) -> Result<AlertPolicyDelivery, BackendError> {
        validate_finish(&params.lease, params.error.as_deref())?;
        let (status, next, delivered) = match params.outcome {
            AlertDeliveryOutcome::Delivered => ("delivered", None, Some(Utc::now())),
            AlertDeliveryOutcome::Retry => (
                "pending",
                Some(params.retry_at.ok_or_else(|| {
                    BackendError::ConstraintViolation("重试必须提供下次执行时间".into())
                })?),
                None,
            ),
            AlertDeliveryOutcome::Failed => ("failed", None, None),
        };
        if matches!(
            params.outcome,
            AlertDeliveryOutcome::Retry | AlertDeliveryOutcome::Failed
        ) && params.error.is_none()
        {
            return Err(BackendError::ConstraintViolation(
                "失败或重试必须记录错误".into(),
            ));
        }
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(error)?;
        let changed=sqlx::query("UPDATE alert_policy_delivery_state SET next_attempt_at=COALESCE(?1,next_attempt_at),delivered_at=?2,lease_owner=NULL,lease_expires_at=NULL WHERE tenant_id=?3 AND delivery_id=?4 AND lease_owner=?5 AND lease_fence=?6")
            .bind(next.map(|v|v.to_rfc3339())).bind(delivered.map(|v|v.to_rfc3339())).bind(&params.tenant_id).bind(&params.delivery_id).bind(&params.lease.worker_id).bind(params.lease.fence).execute(&mut *tx).await.map_err(error)?;
        if changed.rows_affected() != 1 {
            return Err(BackendError::Conflict("告警投递租约已经失效".into()));
        }
        sqlx::query("UPDATE alert_policy_delivery SET status=?1,last_error=?2,updated_at=?3 WHERE tenant_id=?4 AND id=?5").bind(status).bind(&params.error).bind(Utc::now().to_rfc3339()).bind(&params.tenant_id).bind(&params.delivery_id).execute(&mut *tx).await.map_err(error)?;
        let row=sqlx::query("SELECT d.id,d.recipient_id,d.subject,d.body,d.status,d.attempt_count,d.last_error,d.created_at,s.channel,s.next_attempt_at,s.delivered_at FROM alert_policy_delivery d JOIN alert_policy_delivery_state s ON s.tenant_id=d.tenant_id AND s.delivery_id=d.id WHERE d.tenant_id=?1 AND d.id=?2").bind(&params.tenant_id).bind(&params.delivery_id).fetch_one(&mut *tx).await.map_err(error)?;
        let result = Self::decode_delivery(&row)?;
        tx.commit().await.map_err(error)?;
        Ok(result)
    }
}

fn validate_claim(worker: &str, lease_seconds: u32, max_attempts: u32) -> Result<(), BackendError> {
    if worker.is_empty()
        || worker.len() > 128
        || worker != worker.trim()
        || worker.chars().any(char::is_control)
        || !(5..=300).contains(&lease_seconds)
        || !(1..=100).contains(&max_attempts)
    {
        return Err(BackendError::ConstraintViolation(
            "告警 worker 领取参数无效".into(),
        ));
    }
    Ok(())
}

fn validate_finish(
    lease: &AlertDeliveryLease,
    error_message: Option<&str>,
) -> Result<(), BackendError> {
    if lease.worker_id.is_empty()
        || lease.worker_id.len() > 128
        || lease.fence < 1
        || error_message
            .is_some_and(|value| value.is_empty() || value.len() > 2048 || value.contains('\0'))
    {
        return Err(BackendError::ConstraintViolation(
            "告警 worker 完成参数无效".into(),
        ));
    }
    Ok(())
}
