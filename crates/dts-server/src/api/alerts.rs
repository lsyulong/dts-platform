use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::db::DbPool;
use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
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
pub async fn list(pool: web::Data<DbPool>, req: HttpRequest) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let rows = sqlx::query(
        "SELECT ar.id, ar.name, ar.metric_type, ar.operator, ar.threshold,
         ar.enabled, ar.notify_channels, ar.task_id, ar.created_at,
         st.name as task_name
         FROM alert_rule ar
         LEFT JOIN sync_task st ON ar.task_id = st.id
         WHERE ar.tenant_id = $1
         ORDER BY ar.created_at DESC",
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?;

    let alerts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get(8);
            serde_json::json!({
                "id": r.get::<String, _>(0),
                "name": r.get::<String, _>(1),
                "metric_type": r.get::<String, _>(2),
                "operator": r.get::<String, _>(3),
                "threshold": r.get::<f64, _>(4),
                "enabled": r.get::<bool, _>(5),
                "notify_channels": r.get::<String, _>(6),
                "task_id": r.get::<Option<String>, _>(7),
                "task_name": r.get::<Option<String>, _>(9),
                "created_at": created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": alerts })))
}

/// POST /api/v1/alerts
pub async fn create(
    pool: web::Data<DbPool>,
    req: HttpRequest,
    body: web::Json<CreateAlertRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let id = Uuid::new_v4().to_string();
    let channels = serde_json::to_string(&body.notify_channels.as_ref().unwrap_or(&vec![]))
        .unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO alert_rule (id, tenant_id, task_id, name, metric_type, operator,
         threshold, notify_channels)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
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
    pool: web::Data<DbPool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let alert_id = path.into_inner();

    sqlx::query(
        "UPDATE alert_rule SET enabled = NOT enabled,
         updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&alert_id)
    .bind(&claims.tenant_id)
    .execute(pool.get_ref())
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "告警规则状态已切换"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== CreateAlertRequest Tests ==========

    #[test]
    fn create_alert_request_with_all_fields() {
        let json = serde_json::json!({
            "name": "High Error Rate",
            "task_id": "task-1",
            "metric_type": "error_rate",
            "operator": "gt",
            "threshold": 5.0,
            "notify_channels": ["email", "webhook"]
        });
        let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "High Error Rate");
        assert_eq!(req.task_id, Some("task-1".into()));
        assert_eq!(req.metric_type, "error_rate");
        assert_eq!(req.operator, "gt");
        assert_eq!(req.threshold, 5.0);
        assert_eq!(
            req.notify_channels,
            Some(vec!["email".into(), "webhook".into()])
        );
    }

    #[test]
    fn create_alert_request_minimal_fields() {
        let json = serde_json::json!({
            "name": "Simple Alert",
            "metric_type": "latency",
            "operator": "gte",
            "threshold": 100.0
        });
        let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Simple Alert");
        assert_eq!(req.metric_type, "latency");
        assert_eq!(req.operator, "gte");
        assert_eq!(req.threshold, 100.0);
        assert_eq!(req.task_id, None);
        assert_eq!(req.notify_channels, None);
    }

    #[test]
    fn create_alert_request_missing_name_fails() {
        let json = serde_json::json!({
            "metric_type": "error_rate",
            "operator": "gt",
            "threshold": 5.0
        });
        let result = serde_json::from_value::<CreateAlertRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_alert_request_missing_metric_type_fails() {
        let json = serde_json::json!({
            "name": "Alert",
            "operator": "gt",
            "threshold": 5.0
        });
        let result = serde_json::from_value::<CreateAlertRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_alert_request_missing_operator_fails() {
        let json = serde_json::json!({
            "name": "Alert",
            "metric_type": "error_rate",
            "threshold": 5.0
        });
        let result = serde_json::from_value::<CreateAlertRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_alert_request_missing_threshold_fails() {
        let json = serde_json::json!({
            "name": "Alert",
            "metric_type": "error_rate",
            "operator": "gt"
        });
        let result = serde_json::from_value::<CreateAlertRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_alert_request_with_null_optionals() {
        let json = serde_json::json!({
            "name": "Alert",
            "metric_type": "rows_synced",
            "operator": "lt",
            "threshold": 10.0,
            "task_id": null,
            "notify_channels": null
        });
        let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.task_id, None);
        assert_eq!(req.notify_channels, None);
    }

    #[test]
    fn create_alert_request_empty_notify_channels() {
        let json = serde_json::json!({
            "name": "Alert",
            "metric_type": "rows_synced",
            "operator": "eq",
            "threshold": 0.0,
            "notify_channels": []
        });
        let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.notify_channels, Some(vec![]));
    }

    #[test]
    fn create_alert_request_various_operators() {
        for op in &["gt", "gte", "lt", "lte", "eq", "neq"] {
            let json = serde_json::json!({
                "name": format!("{} alert", op),
                "metric_type": "error_rate",
                "operator": op,
                "threshold": 1.0
            });
            let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.operator, *op);
        }
    }

    #[test]
    fn create_alert_request_metric_types() {
        for metric in &["error_rate", "latency", "rows_synced", "bytes_synced"] {
            let json = serde_json::json!({
                "name": "Test",
                "metric_type": metric,
                "operator": "gt",
                "threshold": 0.0
            });
            let req: CreateAlertRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.metric_type, *metric);
        }
    }
}
