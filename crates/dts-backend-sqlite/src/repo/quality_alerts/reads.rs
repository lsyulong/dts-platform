use super::{SqliteQualityAlertRepo, storage::*};
use chrono::{DateTime, Utc};
use dts_backend::{BackendError, repo::quality_alerts::*};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

pub(super) async fn observations(
    pool: &SqlitePool,
    f: &QualityAlertFilter,
) -> Result<QualityPage<QualityObservation>, BackendError> {
    validation::filter(f)?;
    let mut tx = pool.begin().await.map_err(error)?;
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quality_observation WHERE tenant_id=?1 AND monitor_id=?2",
    )
    .bind(&f.tenant_id)
    .bind(&f.monitor_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(error)?;
    let rows:Vec<String>=sqlx::query_scalar("SELECT payload FROM quality_observation WHERE tenant_id=?1 AND monitor_id=?2 ORDER BY created_at DESC,id LIMIT ?3 OFFSET ?4").bind(&f.tenant_id).bind(&f.monitor_id).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(&mut *tx).await.map_err(error)?;
    let data = rows
        .iter()
        .map(|value| decode(value))
        .collect::<Result<_, _>>()?;
    tx.commit().await.map_err(error)?;
    Ok(QualityPage { data, total })
}
pub(super) async fn alerts(
    pool: &SqlitePool,
    f: &QualityAlertFilter,
) -> Result<QualityPage<QualityAlert>, BackendError> {
    validation::filter(f)?;
    let mut tx = pool.begin().await.map_err(error)?;
    let total=sqlx::query_scalar("SELECT COUNT(*) FROM quality_alert WHERE tenant_id=?1 AND monitor_id=?2 AND (?3 IS NULL OR status=?3)").bind(&f.tenant_id).bind(&f.monitor_id).bind(&f.status).fetch_one(&mut *tx).await.map_err(error)?;
    let rows:Vec<String>=sqlx::query_scalar("SELECT payload FROM quality_alert WHERE tenant_id=?1 AND monitor_id=?2 AND (?3 IS NULL OR status=?3) ORDER BY updated_at DESC,id LIMIT ?4 OFFSET ?5").bind(&f.tenant_id).bind(&f.monitor_id).bind(&f.status).bind(i64::from(f.limit)).bind(i64::from(f.offset)).fetch_all(&mut *tx).await.map_err(error)?;
    let data = rows
        .iter()
        .map(|value| decode(value))
        .collect::<Result<_, _>>()?;
    tx.commit().await.map_err(error)?;
    Ok(QualityPage { data, total })
}
pub(super) async fn events(
    pool: &SqlitePool,
    tenant: &str,
    alert: &str,
    limit: u32,
    offset: u32,
) -> Result<QualityPage<QualityAlertEvent>, BackendError> {
    validation::pagination(limit, offset)?;
    let mut tx = pool.begin().await.map_err(error)?;
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quality_alert_event WHERE tenant_id=?1 AND alert_id=?2",
    )
    .bind(tenant)
    .bind(alert)
    .fetch_one(&mut *tx)
    .await
    .map_err(error)?;
    let rows:Vec<String>=sqlx::query_scalar("SELECT payload FROM quality_alert_event WHERE tenant_id=?1 AND alert_id=?2 ORDER BY created_at DESC,id LIMIT ?3 OFFSET ?4").bind(tenant).bind(alert).bind(i64::from(limit)).bind(i64::from(offset)).fetch_all(&mut *tx).await.map_err(error)?;
    let data = rows
        .iter()
        .map(|value| decode(value))
        .collect::<Result<_, _>>()?;
    tx.commit().await.map_err(error)?;
    Ok(QualityPage { data, total })
}
fn notice(row: &SqliteRow) -> Result<QualityAlertNotice, BackendError> {
    let parse = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|date| date.with_timezone(&Utc))
            .map_err(|_| BackendError::Internal("告警提醒时间无效".into()))
    };
    Ok(QualityAlertNotice {
        id: row.try_get("id").map_err(error)?,
        tenant_id: row.try_get("tenant_id").map_err(error)?,
        alert_id: row.try_get("alert_id").map_err(error)?,
        recipient_id: row.try_get("recipient_id").map_err(error)?,
        subject: row.try_get("subject").map_err(error)?,
        link: row.try_get("link").map_err(error)?,
        delivery_status: row.try_get("delivery_status").map_err(error)?,
        read_at: row
            .try_get::<Option<String>, _>("read_at")
            .map_err(error)?
            .map(parse)
            .transpose()?,
        created_at: parse(row.try_get("created_at").map_err(error)?)?,
    })
}
pub(super) async fn notices(
    pool: &SqlitePool,
    tenant: &str,
    recipient: &str,
    unread: bool,
    limit: u32,
    offset: u32,
) -> Result<QualityPage<QualityAlertNotice>, BackendError> {
    validation::pagination(limit, offset)?;
    let mut tx = pool.begin().await.map_err(error)?;
    active(&mut tx, tenant, recipient).await?;
    let total=sqlx::query_scalar("SELECT COUNT(*) FROM quality_alert_notice WHERE tenant_id=?1 AND recipient_id=?2 AND (?3=0 OR read_at IS NULL)").bind(tenant).bind(recipient).bind(unread).fetch_one(&mut *tx).await.map_err(error)?;
    let rows=sqlx::query("SELECT * FROM quality_alert_notice WHERE tenant_id=?1 AND recipient_id=?2 AND (?3=0 OR read_at IS NULL) ORDER BY created_at DESC,id LIMIT ?4 OFFSET ?5").bind(tenant).bind(recipient).bind(unread).bind(i64::from(limit)).bind(i64::from(offset)).fetch_all(&mut *tx).await.map_err(error)?;
    let data = rows.iter().map(notice).collect::<Result<_, _>>()?;
    tx.commit().await.map_err(error)?;
    Ok(QualityPage { data, total })
}
pub(super) async fn mark_read(
    repo: &SqliteQualityAlertRepo,
    tenant: &str,
    recipient: &str,
    id: &str,
) -> Result<QualityAlertNotice, BackendError> {
    let mut tx = repo.begin_write(tenant).await?;
    active(&mut tx, tenant, recipient).await?;
    sqlx::query("UPDATE quality_alert_notice SET read_at=COALESCE(read_at,?1) WHERE tenant_id=?2 AND recipient_id=?3 AND id=?4").bind(now().to_rfc3339()).bind(tenant).bind(recipient).bind(id).execute(&mut *tx).await.map_err(error)?;
    let row = sqlx::query(
        "SELECT * FROM quality_alert_notice WHERE tenant_id=?1 AND recipient_id=?2 AND id=?3",
    )
    .bind(tenant)
    .bind(recipient)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(error)?
    .ok_or_else(|| BackendError::NotFound("本人告警提醒不存在".into()))?;
    let result = notice(&row)?;
    tx.commit().await.map_err(error)?;
    Ok(result)
}
