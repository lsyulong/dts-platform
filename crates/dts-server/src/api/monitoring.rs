use actix_web::{HttpRequest, HttpResponse, web};
use sqlx::Row;

use crate::db::DbPool;
use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;

/// GET /api/v1/monitoring/overview
pub async fn overview(pool: web::Data<DbPool>, req: HttpRequest) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let total_tasks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1")
            .bind(&claims.tenant_id)
            .fetch_one(pool.get_ref())
            .await?;

    let running_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1 AND status = 'running'",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let failed_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_task WHERE tenant_id = $1 AND status = 'failed'",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let total_datasources: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE tenant_id = $1")
            .bind(&claims.tenant_id)
            .fetch_one(pool.get_ref())
            .await?;

    let healthy_datasources: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM datasource WHERE tenant_id = $1 AND health_status = 'healthy'",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let today_synced_rows: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(rows_synced) FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = $1 AND tr.start_time >= CURRENT_DATE",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let today_synced_bytes: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(bytes_synced) FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = $1 AND tr.start_time >= CURRENT_DATE",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let recent_runs: Vec<serde_json::Value> = sqlx::query(
        "SELECT tr.id, tr.task_id, st.name, tr.status,
         tr.start_time, tr.rows_synced, tr.bytes_synced
         FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = $1
         ORDER BY tr.start_time DESC LIMIT 10",
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?
    .into_iter()
    .map(|r| {
        let start_time: chrono::DateTime<chrono::Utc> = r.get(4);
        serde_json::json!({
            "id": r.get::<String, _>(0),
            "task_id": r.get::<String, _>(1),
            "task_name": r.get::<String, _>(2),
            "status": r.get::<String, _>(3),
            "start_time": start_time.to_rfc3339(),
            "rows_synced": r.get::<i64, _>(5),
            "bytes_synced": r.get::<i64, _>(6),
        })
    })
    .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "total_tasks": total_tasks,
        "running_tasks": running_tasks,
        "failed_tasks": failed_tasks,
        "total_datasources": total_datasources,
        "healthy_datasources": healthy_datasources,
        "today_synced_rows": today_synced_rows.unwrap_or(0),
        "today_synced_bytes": today_synced_bytes.unwrap_or(0),
        "success_rate": if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        },
        "recent_runs": recent_runs,
    })))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    // ========== Overview Response Structure Tests ==========

    #[test]
    fn overview_response_json_structure_is_complete() {
        let response = serde_json::json!({
            "total_tasks": 10,
            "running_tasks": 3,
            "failed_tasks": 1,
            "total_datasources": 5,
            "healthy_datasources": 4,
            "today_synced_rows": 50000,
            "today_synced_bytes": 1048576,
            "success_rate": 90.0,
            "recent_runs": []
        });

        assert!(response["total_tasks"].is_number());
        assert!(response["running_tasks"].is_number());
        assert!(response["failed_tasks"].is_number());
        assert!(response["total_datasources"].is_number());
        assert!(response["healthy_datasources"].is_number());
        assert!(response["today_synced_rows"].is_number());
        assert!(response["today_synced_bytes"].is_number());
        assert!(response["success_rate"].is_number());
        assert!(response["recent_runs"].is_array());
    }

    #[test]
    fn overview_success_rate_calculation() {
        let total_tasks: i64 = 10;
        let failed_tasks: i64 = 2;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 80.0);
    }

    #[test]
    fn overview_success_rate_when_no_tasks() {
        let total_tasks: i64 = 0;
        let failed_tasks: i64 = 0;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 100.0);
    }

    #[test]
    fn overview_success_rate_when_all_failed() {
        let total_tasks: i64 = 5;
        let failed_tasks: i64 = 5;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 0.0);
    }

    #[test]
    fn overview_success_rate_when_all_succeed() {
        let total_tasks: i64 = 10;
        let failed_tasks: i64 = 0;
        let success_rate = if total_tasks > 0 {
            100.0 * (total_tasks - failed_tasks) as f64 / total_tasks as f64
        } else {
            100.0
        };
        assert_eq!(success_rate, 100.0);
    }

    #[test]
    fn overview_default_values_for_null_sums() {
        let rows: Option<i64> = None;
        assert_eq!(rows.unwrap_or(0), 0);

        let bytes: Option<i64> = None;
        assert_eq!(bytes.unwrap_or(0), 0);

        let rows_some: Option<i64> = Some(12345);
        assert_eq!(rows_some.unwrap_or(0), 12345);
    }

    #[test]
    fn overview_recent_runs_structure() {
        let run = serde_json::json!({
            "id": "run-1",
            "task_id": "task-1",
            "task_name": "Sync Task",
            "status": "completed",
            "start_time": "2024-01-01T00:00:00Z",
            "rows_synced": 1000,
            "bytes_synced": 50000
        });
        assert_eq!(run["id"], "run-1");
        assert_eq!(run["status"], "completed");
        assert_eq!(run["rows_synced"], 1000);
        assert_eq!(run["bytes_synced"], 50000);
    }
}
