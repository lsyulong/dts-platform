// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::middleware::auth;
use crate::models::error::AppError;
use crate::services::alert_svc::AlertService;

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
    alert_svc: web::Data<AlertService>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "alert:read").await?;

    let alerts = alert_svc.list_by_tenant(&claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": alerts })))
}

/// POST /api/v1/alerts
pub async fn create(
    alert_svc: web::Data<AlertService>,
    req: HttpRequest,
    body: web::Json<CreateAlertRequest>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "alert:write").await?;

    if let Some(task_id) = &body.task_id {
        let repo = req
            .app_data::<web::Data<std::sync::Arc<dyn dts_backend::TaskRepo>>>()
            .ok_or_else(|| AppError::Internal("任务存储未配置".into()))?;
        repo.find_detail_by_id(task_id, &claims.tenant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;
    }

    let channels = serde_json::to_string(&body.notify_channels.as_ref().unwrap_or(&vec![]))
        .unwrap_or_else(|_| "[]".to_string());

    let id = alert_svc
        .create(
            &claims.tenant_id,
            body.task_id.as_deref(),
            &body.name,
            &body.metric_type,
            &body.operator,
            body.threshold,
            &channels,
        )
        .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
    })))
}

/// PATCH /api/v1/alerts/{id}/toggle
pub async fn toggle(
    alert_svc: web::Data<AlertService>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "alert:write").await?;
    let alert_id = path.into_inner();

    alert_svc.toggle(&alert_id, &claims.tenant_id).await?;

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
