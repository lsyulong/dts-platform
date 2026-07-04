use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::middleware::auth;
use crate::models::error::AppError;

/// 创建告警规则请求
#[derive(Debug, Deserialize)]
pub struct CreateAlertRequest {
    pub name: String,
    pub task_id: Option<String>,
    pub metric_type: String,
    pub operator: String,
    pub threshold: f64,
    pub notify_channels: Option<Vec<String>>,
}

/// GET /api/v1/alerts
pub async fn list(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let rows = sqlx::query(
        "SELECT ar.id, ar.name, ar.metric_type, ar.operator, ar.threshold,
         ar.enabled, ar.notify_channels, ar.task_id, ar.created_at,
         st.name as task_name
         FROM alert_rule ar
         LEFT JOIN sync_task st ON ar.task_id = st.id
         WHERE ar.tenant_id = ?
         ORDER BY ar.created_at DESC"
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?;

    let alerts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let enabled: i64 = r.get(5);
            serde_json::json!({
                "id": r.get::<String, _>(0),
                "name": r.get::<String, _>(1),
                "metric_type": r.get::<String, _>(2),
                "operator": r.get::<String, _>(3),
                "threshold": r.get::<f64, _>(4),
                "enabled": enabled != 0,
                "notify_channels": r.get::<String, _>(6),
                "task_id": r.get::<Option<String>, _>(7),
                "task_name": r.get::<Option<String>, _>(9),
                "created_at": r.get::<String, _>(8),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": alerts })))
}

/// POST /api/v1/alerts
pub async fn create(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    body: web::Json<CreateAlertRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let id = Uuid::new_v4().to_string();
    let channels = serde_json::to_string(&body.notify_channels.as_ref().unwrap_or(&vec![]))
        .unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO alert_rule (id, tenant_id, task_id, name, metric_type, operator,
         threshold, notify_channels)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&claims.tenant_id)
    .bind(&body.task_id)
    .bind(&body.name)
    .bind(&body.metric_type)
    .bind(&body.operator)
    .bind(body.threshold)
    .bind(&channels)
    .execute(pool.get_ref())
    .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
    })))
}

/// PATCH /api/v1/alerts/{id}/toggle
pub async fn toggle(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let alert_id = path.into_inner();

    sqlx::query(
        "UPDATE alert_rule SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END,
         updated_at = datetime('now')
         WHERE id = ? AND tenant_id = ?"
    )
    .bind(&alert_id)
    .bind(&claims.tenant_id)
    .execute(pool.get_ref())
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "告警规则状态已切换"
    })))
}

fn get_jwt_secret(_req: &HttpRequest) -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string())
}
