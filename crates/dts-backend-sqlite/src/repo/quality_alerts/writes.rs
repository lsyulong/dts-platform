use super::{SqliteQualityAlertRepo, storage::*, verify};
use dts_backend::{BackendError, repo::quality_alerts::*};
use sqlx::SqliteConnection;

pub(super) async fn record(
    repo: &SqliteQualityAlertRepo,
    params: &RecordQualityEvaluationParams,
) -> Result<QualityEvaluation, BackendError> {
    let semantics = validation::evaluation(params)?;
    let observation = &params.observation;
    let mut tx = repo.begin_write(&observation.tenant_id).await?;
    authority(&mut tx, &observation.tenant_id, &observation.actor_id, true).await?;
    if let Some(mut replay) = receipt::<QualityEvaluation>(
        &mut tx,
        &observation.tenant_id,
        &observation.actor_id,
        &params.idempotency_key,
        &semantics,
    )
    .await?
    {
        replay.replayed = true;
        tx.commit().await.map_err(error)?;
        return Ok(replay);
    }
    verify::observation(&mut tx, observation).await?;
    for recipient in &params.recipient_ids {
        let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" WHERE tenant_id=?1 AND id=?2 AND status='active')").bind(&observation.tenant_id).bind(recipient).fetch_one(&mut *tx).await.map_err(error)?;
        if !active {
            return Err(BackendError::Forbidden(
                "告警收件人不存在、已停用或不属于当前租户".into(),
            ));
        }
    }
    sqlx::query("INSERT INTO quality_observation(id,tenant_id,monitor_id,monitor_version,object_id,actor_id,period_start,period_end,state,payload,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)").bind(&observation.id).bind(&observation.tenant_id).bind(&observation.monitor_id).bind(observation.monitor_version).bind(&observation.object_id).bind(&observation.actor_id).bind(observation.period_start.to_rfc3339()).bind(observation.period_end.to_rfc3339()).bind(&observation.state).bind(encode(observation)?).bind(observation.created_at.to_rfc3339()).execute(&mut *tx).await.map_err(error)?;
    for resource in &observation.snapshot.resources {
        sqlx::query("INSERT INTO quality_observation_resource(tenant_id,observation_id,kind,resource_id) VALUES(?1,?2,?3,?4)").bind(&observation.tenant_id).bind(&observation.id).bind(&resource.kind).bind(&resource.id).execute(&mut *tx).await.map_err(error)?;
    }
    for job in &observation.snapshot.jobs {
        sqlx::query(
            "INSERT INTO quality_observation_job(tenant_id,observation_id,job_id) VALUES(?1,?2,?3)",
        )
        .bind(&observation.tenant_id)
        .bind(&observation.id)
        .bind(&job.id)
        .execute(&mut *tx)
        .await
        .map_err(error)?;
    }
    let value:Option<String>=sqlx::query_scalar("SELECT payload FROM quality_alert WHERE tenant_id=?1 AND monitor_id=?2 AND status IN('open','acknowledged')").bind(&observation.tenant_id).bind(&observation.monitor_id).fetch_optional(&mut *tx).await.map_err(error)?;
    let mut active = value.as_deref().map(decode::<QualityAlert>).transpose()?;
    // A new policy version must be evaluated independently; an unavailable
    // observation never conceals an existing unresolved incident.
    let state = &observation.state;
    if state == "threshold_exceeded" {
        let opening = active.is_none();
        let before = active.as_ref().map(|alert| alert.status.clone());
        let old_version = active.as_ref().map(|alert| alert.version);
        let alert = active.get_or_insert_with(|| QualityAlert {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: observation.tenant_id.clone(),
            monitor_id: observation.monitor_id.clone(),
            monitor_version: observation.monitor_version,
            status: "open".into(),
            version: 0,
            opening_observation_id: observation.id.clone(),
            last_observation_id: observation.id.clone(),
            acknowledged_by: None,
            acknowledged_at: None,
            resolved_at: None,
            created_at: observation.created_at,
            updated_at: observation.created_at,
        });
        alert.version = alert
            .version
            .checked_add(1)
            .ok_or_else(|| conflict("告警版本已耗尽"))?;
        alert.monitor_version = observation.monitor_version;
        alert.last_observation_id = observation.id.clone();
        alert.updated_at = observation.created_at;
        save_alert(&mut tx, alert, old_version).await?;
        append_event(
            &mut tx,
            observation,
            alert,
            if opening { "open" } else { "observe" },
            before,
            "threshold_exceeded",
        )
        .await?;
        if opening {
            notices(&mut tx, params, alert).await?;
        }
    } else if state == "within_threshold" {
        if let Some(alert) = active.as_mut() {
            // Changing the threshold is not evidence that the old incident recovered.
            if alert.monitor_version == observation.monitor_version {
                let before = Some(alert.status.clone());
                let version = alert.version;
                alert.status = "resolved".into();
                alert.version = version
                    .checked_add(1)
                    .ok_or_else(|| conflict("告警版本已耗尽"))?;
                alert.last_observation_id = observation.id.clone();
                alert.resolved_at = Some(observation.created_at);
                alert.updated_at = observation.created_at;
                save_alert(&mut tx, alert, Some(version)).await?;
                append_event(
                    &mut tx,
                    observation,
                    alert,
                    "resolve",
                    before,
                    "within_threshold",
                )
                .await?;
            } else {
                let version = alert.version;
                alert.version = version
                    .checked_add(1)
                    .ok_or_else(|| conflict("告警版本已耗尽"))?;
                alert.last_observation_id = observation.id.clone();
                alert.updated_at = observation.created_at;
                save_alert(&mut tx, alert, Some(version)).await?;
                append_event(
                    &mut tx,
                    observation,
                    alert,
                    "observe",
                    Some(alert.status.clone()),
                    "policy_changed",
                )
                .await?;
            }
        }
    } else if let Some(alert) = active.as_mut() {
        let version = alert.version;
        alert.version = version
            .checked_add(1)
            .ok_or_else(|| conflict("告警版本已耗尽"))?;
        alert.last_observation_id = observation.id.clone();
        alert.updated_at = observation.created_at;
        save_alert(&mut tx, alert, Some(version)).await?;
        append_event(
            &mut tx,
            observation,
            alert,
            "unavailable",
            Some(alert.status.clone()),
            &observation.reason,
        )
        .await?;
    }
    let result = QualityEvaluation {
        observation: observation.clone(),
        alert: active,
        replayed: false,
    };
    save_receipt(
        &mut tx,
        &observation.tenant_id,
        &observation.actor_id,
        &params.idempotency_key,
        &semantics,
        &result,
    )
    .await?;
    audit(
        &mut tx,
        &observation.tenant_id,
        &observation.actor_id,
        &observation.monitor_id,
        "evaluate",
        &observation.id,
        None,
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}

async fn append_event(
    conn: &mut SqliteConnection,
    observation: &QualityObservation,
    alert: &QualityAlert,
    action: &str,
    before: Option<String>,
    reason: &str,
) -> Result<(), BackendError> {
    event(
        conn,
        &QualityAlertEvent {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: alert.tenant_id.clone(),
            alert_id: alert.id.clone(),
            observation_id: observation.id.clone(),
            actor_id: observation.actor_id.clone(),
            action: action.into(),
            reason: reason.into(),
            before_status: before,
            after_status: alert.status.clone(),
            created_at: observation.created_at,
        },
    )
    .await
}
async fn notices(
    conn: &mut SqliteConnection,
    params: &RecordQualityEvaluationParams,
    alert: &QualityAlert,
) -> Result<(), BackendError> {
    for recipient in &params.recipient_ids {
        sqlx::query("INSERT INTO quality_alert_notice(id,tenant_id,alert_id,recipient_id,subject,link,delivery_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)").bind(uuid::Uuid::new_v4().to_string()).bind(&alert.tenant_id).bind(&alert.id).bind(recipient).bind(&params.notice_subject).bind("/quality/monitoring").bind(alert.created_at.to_rfc3339()).execute(&mut *conn).await.map_err(error)?;
    }
    Ok(())
}
pub(super) async fn acknowledge(
    repo: &SqliteQualityAlertRepo,
    params: &AcknowledgeQualityAlertParams,
) -> Result<QualityAlert, BackendError> {
    let semantics = validation::ack_semantics(params)?;
    let mut tx = repo.begin_write(&params.tenant_id).await?;
    authority(&mut tx, &params.tenant_id, &params.actor_id, true).await?;
    if let Some(replay) = receipt::<QualityAlert>(
        &mut tx,
        &params.tenant_id,
        &params.actor_id,
        &params.idempotency_key,
        &semantics,
    )
    .await?
    {
        tx.commit().await.map_err(error)?;
        return Ok(replay);
    }
    let mut alert = alert(&mut tx, &params.tenant_id, &params.alert_id)
        .await?
        .ok_or_else(|| BackendError::NotFound("质量告警不存在".into()))?;
    if alert.version != params.expected_version || alert.status != "open" {
        return Err(conflict("只有当前版本未确认告警可以确认"));
    }
    let before = alert.status.clone();
    alert.status = "acknowledged".into();
    alert.version = alert
        .version
        .checked_add(1)
        .ok_or_else(|| conflict("告警版本已耗尽"))?;
    alert.acknowledged_by = Some(params.actor_id.clone());
    let acknowledged_at = now();
    alert.acknowledged_at = Some(acknowledged_at);
    alert.updated_at = acknowledged_at;
    save_alert(&mut tx, &alert, Some(params.expected_version)).await?;
    event(
        &mut tx,
        &QualityAlertEvent {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: params.tenant_id.clone(),
            alert_id: alert.id.clone(),
            observation_id: alert.last_observation_id.clone(),
            actor_id: params.actor_id.clone(),
            action: "acknowledge".into(),
            reason: params.reason.clone(),
            before_status: Some(before),
            after_status: alert.status.clone(),
            created_at: alert.updated_at,
        },
    )
    .await?;
    save_receipt(
        &mut tx,
        &params.tenant_id,
        &params.actor_id,
        &params.idempotency_key,
        &semantics,
        &alert,
    )
    .await?;
    audit(
        &mut tx,
        &params.tenant_id,
        &params.actor_id,
        &alert.monitor_id,
        "acknowledge",
        &alert.last_observation_id,
        Some(&alert.id),
    )
    .await?;
    tx.commit().await.map_err(error)?;
    Ok(alert)
}
async fn audit(
    conn: &mut SqliteConnection,
    tenant: &str,
    actor: &str,
    monitor: &str,
    action: &str,
    observation: &str,
    alert: Option<&str>,
) -> Result<(), BackendError> {
    sqlx::query("INSERT INTO product_resource_audit(id,tenant_id,actor_id,resource_kind,resource_id,operation,summary,created_at) VALUES(?1,?2,?3,'quality_monitor',?4,'update',?5,?6)").bind(uuid::Uuid::new_v4().to_string()).bind(tenant).bind(actor).bind(monitor).bind(encode(&serde_json::json!({"action":action,"observation_id":observation,"alert_id":alert}))?).bind(now().to_rfc3339()).execute(conn).await.map_err(error)?;
    Ok(())
}
