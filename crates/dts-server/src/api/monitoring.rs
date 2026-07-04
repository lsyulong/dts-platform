use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::{Row, SqlitePool};

use crate::middleware::auth;
use crate::models::error::AppError;

/// GET /api/v1/monitoring/overview
pub async fn overview(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let total_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_task WHERE tenant_id = ?"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let running_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_task WHERE tenant_id = ? AND status = 'running'"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let failed_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_task WHERE tenant_id = ? AND status = 'failed'"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let total_datasources: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM datasource WHERE tenant_id = ?"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let healthy_datasources: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM datasource WHERE tenant_id = ? AND health_status = 'healthy'"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let today_synced_rows: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(rows_synced) FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = ? AND tr.start_time >= date('now')"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let today_synced_bytes: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(bytes_synced) FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = ? AND tr.start_time >= date('now')"
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool.get_ref())
    .await?;

    let recent_runs: Vec<serde_json::Value> = sqlx::query(
        "SELECT tr.id, tr.task_id, st.name, tr.status,
         tr.start_time, tr.rows_synced, tr.bytes_synced
         FROM task_run tr
         JOIN sync_task st ON tr.task_id = st.id
         WHERE st.tenant_id = ?
         ORDER BY tr.start_time DESC LIMIT 10"
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?
    .into_iter()
    .map(|r| {
        serde_json::json!({
            "id": r.get::<String, _>(0),
            "task_id": r.get::<String, _>(1),
            "task_name": r.get::<String, _>(2),
            "status": r.get::<String, _>(3),
            "start_time": r.get::<String, _>(4),
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

fn get_jwt_secret(_req: &HttpRequest) -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string())
}
