use async_trait::async_trait;
use dts_backend::{BackendError, repo::AlertRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgAlertRepo {
    pool: PgPool,
}

impl PgAlertRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AlertRepo for PgAlertRepo {
    async fn create(&self, params: &CreateAlertParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO alert_rule (id, tenant_id, task_id, name, metric_type, operator,
             threshold, notify_channels)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.task_id)
        .bind(&params.name)
        .bind(&params.metric_type)
        .bind(&params.operator)
        .bind(params.threshold)
        .bind(&params.notify_channels)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<AlertInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT ar.id, ar.name, ar.metric_type, ar.operator, ar.threshold,
             ar.enabled, ar.notify_channels, ar.task_id, ar.created_at,
             st.name as task_name
             FROM alert_rule ar
             LEFT JOIN sync_task st ON ar.task_id = st.id
             WHERE ar.tenant_id = $1
             ORDER BY ar.created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let alerts = rows
            .into_iter()
            .map(|r| {
                let created_at: chrono::DateTime<chrono::Utc> = r.get(8);
                AlertInfo {
                    id: r.get(0),
                    name: r.get(1),
                    metric_type: r.get(2),
                    operator: r.get(3),
                    threshold: r.get(4),
                    enabled: r.get(5),
                    notify_channels: r.get(6),
                    task_id: r.get(7),
                    task_name: r.get(9),
                    created_at: created_at.to_rfc3339(),
                }
            })
            .collect();

        Ok(alerts)
    }

    async fn toggle(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        let result = sqlx::query(
            "UPDATE alert_rule SET enabled = NOT enabled, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }
}
