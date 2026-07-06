use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use uuid::Uuid;

use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;
use dts_backend::repo::AlertRepo;
use dts_backend::types::CreateAlertParams;

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
    alert_repo: web::Data<Arc<dyn AlertRepo>>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let alerts = alert_repo.list_by_tenant(&claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": alerts })))
}

/// POST /api/v1/alerts
pub async fn create(
    alert_repo: web::Data<Arc<dyn AlertRepo>>,
    req: HttpRequest,
    body: web::Json<CreateAlertRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let id = Uuid::new_v4().to_string();
    let channels = serde_json::to_string(&body.notify_channels.as_ref().unwrap_or(&vec![]))
        .unwrap_or_else(|_| "[]".to_string());

    alert_repo
        .create(&CreateAlertParams {
            id: id.clone(),
            tenant_id: claims.tenant_id,
            task_id: body.task_id.clone(),
            name: body.name.clone(),
            metric_type: body.metric_type.clone(),
            operator: body.operator.clone(),
            threshold: body.threshold,
            notify_channels: channels,
        })
        .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
    })))
}

/// PATCH /api/v1/alerts/{id}/toggle
pub async fn toggle(
    alert_repo: web::Data<Arc<dyn AlertRepo>>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let alert_id = path.into_inner();

    alert_repo.toggle(&alert_id, &claims.tenant_id).await?;

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
        assert_eq!(req.metric_type, "latency");
        assert_eq!(req.operator, "gte");
        assert_eq!(req.threshold, 100.0);
        assert_eq!(req.task_id, None);
        assert_eq!(req.notify_channels, None);
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
}
