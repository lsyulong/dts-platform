use async_trait::async_trait;
use chrono::Utc;
use dts_backend::{BackendError, repo::platform_projection::*};
use sqlx::{PgConnection, PgPool, Row};
mod queries;

pub struct PgPlatformProjectionRepo {
    pool: PgPool,
}
impl PgPlatformProjectionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
fn error(_: sqlx::Error) -> BackendError {
    BackendError::Query("平台投影来源读取失败".into())
}
async fn authority(
    conn: &mut PgConnection,
    tenant: &str,
    actor: &str,
) -> Result<ProjectionAuthority, BackendError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u JOIN tenant t ON t.id=u.tenant_id WHERE t.id=$1 AND u.id=$2 AND t.status='active' AND u.status='active')")
        .bind(tenant).bind(actor).fetch_one(&mut *conn).await.map_err(error)?;
    if !active {
        return Err(BackendError::Forbidden("当前用户或租户已停用".into()));
    }
    let permissions:Vec<String>=sqlx::query_scalar("SELECT DISTINCT rp.permission FROM user_role ur JOIN role r ON r.id=ur.role_id JOIN role_permission rp ON rp.role_id=r.id WHERE ur.user_id=$1 AND r.tenant_id=$2 ORDER BY rp.permission")
        .bind(actor).bind(tenant).fetch_all(conn).await.map_err(error)?;
    Ok(ProjectionAuthority { permissions })
}
async fn check(
    conn: &mut PgConnection,
    p: &ProjectionRequest,
    needs: &[&str],
) -> Result<(), BackendError> {
    validate_request(p)?;
    let current = authority(conn, &p.tenant_id, &p.actor_id).await?;
    if current.permissions != p.expected_permissions || !permitted(&current.permissions, needs) {
        return Err(BackendError::Forbidden("投影来源权限不足或已经改变".into()));
    }
    Ok(())
}
#[async_trait]
impl PlatformProjectionRepo for PgPlatformProjectionRepo {
    async fn authority(
        &self,
        tenant: &str,
        actor: &str,
    ) -> Result<ProjectionAuthority, BackendError> {
        let mut conn = self.pool.acquire().await.map_err(error)?;
        authority(&mut conn, tenant, actor).await
    }
    async fn work_source(
        &self,
        p: &ProjectionRequest,
        source: WorkSource,
    ) -> Result<WorkSourcePage, BackendError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        check(&mut tx, p, source.permissions()).await?;
        if matches!(
            source,
            WorkSource::QueuedJobs | WorkSource::ReconciliationJobs
        ) && visible_kinds(p, true).is_empty()
        {
            return Err(BackendError::Forbidden("当前没有可执行的来源域".into()));
        }
        let kinds = serde_json::to_string(&visible_kinds(p, true))
            .map_err(|_| BackendError::Internal("类型过滤无法编码".into()))?;
        let (from, fields) = queries::work(source);
        let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {from}"))
            .bind(&p.tenant_id)
            .bind(&p.actor_id)
            .bind(&kinds)
            .fetch_one(&mut *tx)
            .await
            .map_err(error)?;
        let rows = sqlx::query(&format!(
            "SELECT {fields} {from} ORDER BY created_at DESC,id LIMIT $4 OFFSET $5"
        ))
        .bind(&p.tenant_id)
        .bind(&p.actor_id)
        .bind(&kinds)
        .bind(i64::from(p.limit))
        .bind(i64::from(p.offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(error)?;
        let mut data = Vec::new();
        for row in rows {
            let source_kind: String = row.try_get("source_kind").map_err(error)?;
            let entry = p.kinds.iter().find(|k| k.kind == source_kind);
            let required_permissions = if matches!(
                source,
                WorkSource::QueuedJobs | WorkSource::ReconciliationJobs
            ) {
                entry
                    .map(|k| vec![format!("{}:read", k.scope), format!("{}:execute", k.scope)])
                    .unwrap_or_default()
            } else {
                source.permissions().iter().map(|v| (*v).into()).collect()
            };
            data.push(WorkItem {
                id: row.try_get("id").map_err(error)?,
                source_kind,
                source_id: row.try_get("source_id").map_err(error)?,
                source_version: row.try_get("source_version").map_err(error)?,
                title: row.try_get("title").map_err(error)?,
                status: row.try_get("status").map_err(error)?,
                created_at: row.try_get("created_at").map_err(error)?,
                due_at: row.try_get("due_at").map_err(error)?,
                route: entry.map(|e| e.path.clone()),
                required_permissions,
            });
        }
        tx.commit().await.map_err(error)?;
        Ok(WorkSourcePage {
            observed_at: Utc::now(),
            data,
            total,
        })
    }
    async fn statistics_source(
        &self,
        p: &StatisticsRequest,
        source: StatisticsSource,
    ) -> Result<SourceStatistics, BackendError> {
        let mut tx = self.pool.begin().await.map_err(error)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(error)?;
        check(&mut tx, &p.actor, source.permissions()).await?;
        if p.end <= p.start || p.end - p.start > chrono::Duration::days(366) {
            return Err(BackendError::ConstraintViolation(
                "统计窗口应为正数且不超过366天".into(),
            ));
        }
        if matches!(
            source,
            StatisticsSource::ProductDefinitions | StatisticsSource::ProductJobs
        ) && visible_kinds(&p.actor, false).is_empty()
        {
            return Err(BackendError::Forbidden("当前没有可读取的来源域".into()));
        }
        let kinds = serde_json::to_string(&visible_kinds(&p.actor, false))
            .map_err(|_| BackendError::Internal("类型过滤无法编码".into()))?;
        let (from, dimension, time) = queries::statistics(source);
        let sql = format!(
            "SELECT {dimension} AS dimension,COUNT(*) AS count {from} AND {time}>=$4::timestamptz AND {time}<$5::timestamptz GROUP BY {dimension} ORDER BY dimension"
        );
        let rows = sqlx::query(&sql)
            .bind(&p.actor.tenant_id)
            .bind(&p.actor.actor_id)
            .bind(kinds)
            .bind(p.start)
            .bind(p.end)
            .fetch_all(&mut *tx)
            .await
            .map_err(error)?;
        let data = rows
            .iter()
            .map(|r| {
                Ok(ProjectionCount {
                    dimension: r.try_get("dimension").map_err(error)?,
                    count: r.try_get("count").map_err(error)?,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        let total = data.iter().try_fold(0_i64, |sum, v| {
            sum.checked_add(v.count)
                .ok_or_else(|| BackendError::Internal("统计计数超出范围".into()))
        })?;
        tx.commit().await.map_err(error)?;
        Ok(SourceStatistics {
            observed_at: Utc::now(),
            data,
            total,
        })
    }
}
