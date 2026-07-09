use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};

use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;
use crate::services::task_svc::TaskService;
use dts_backend::repo::TaskRepo;

/// 创建任务请求
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub table_mappings: Option<serde_json::Value>,
    pub filter_config: Option<serde_json::Value>,
}

/// 任务列表响应
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/tasks
pub async fn list(
    task_svc: web::Data<TaskService>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let tasks = task_svc.list_by_tenant(&claims.tenant_id).await?;
    let total = tasks.len();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": tasks,
        "total": total,
    })))
}

/// POST /api/v1/tasks
pub async fn create(
    task_svc: web::Data<TaskService>,
    req: HttpRequest,
    body: web::Json<CreateTaskRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let config_json = serde_json::json!({
        "table_mappings": body.table_mappings,
        "filter_config": body.filter_config,
    });

    let task_id = task_svc
        .create(
            &claims.tenant_id,
            &body.name,
            body.description.as_deref(),
            &body.source_id,
            &body.target_id,
            body.cron_expr.as_deref(),
            &config_json.to_string(),
            &claims.sub,
        )
        .await?;

    tracing::info!("任务创建: {} (租户: {})", body.name, claims.tenant_id);

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": task_id,
        "name": body.name,
        "status": "draft",
    })))
}

/// GET /api/v1/tasks/{id}
pub async fn get(
    task_repo: web::Data<Arc<dyn TaskRepo>>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    let task = task_repo
        .find_detail_by_id(&task_id, &claims.tenant_id)
        .await?
        .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;

    Ok(HttpResponse::Ok().json(task))
}

/// PATCH /api/v1/tasks/{id}/status
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_status(
    task_svc: web::Data<TaskService>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateStatusRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    task_svc
        .update_status(&task_id, &claims.tenant_id, &body.status)
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": task_id,
        "status": body.status,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== CreateTaskRequest Deserialization Tests ==========

    #[test]
    fn create_task_request_with_all_fields() {
        let json = serde_json::json!({
            "name": "My Sync Task",
            "description": "Syncs user data",
            "source_id": "src-1",
            "target_id": "tgt-1",
            "cron_expr": "0 */6 * * *",
            "table_mappings": {"users": "users_backup"},
            "filter_config": {"status": "active"}
        });
        let req: CreateTaskRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "My Sync Task");
        assert_eq!(req.description, Some("Syncs user data".into()));
        assert_eq!(req.source_id, "src-1");
        assert_eq!(req.target_id, "tgt-1");
        assert_eq!(req.cron_expr, Some("0 */6 * * *".into()));
        assert!(req.table_mappings.is_some());
        assert!(req.filter_config.is_some());
    }

    #[test]
    fn create_task_request_minimal_fields() {
        let json = serde_json::json!({
            "name": "Simple Task",
            "source_id": "src-1",
            "target_id": "tgt-1"
        });
        let req: CreateTaskRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Simple Task");
        assert_eq!(req.source_id, "src-1");
        assert_eq!(req.target_id, "tgt-1");
    }

    // ========== UpdateStatusRequest Tests ==========

    #[test]
    fn update_status_request_valid_statuses() {
        for status in &["draft", "running", "paused", "completed", "failed"] {
            let json = serde_json::json!({"status": status});
            let req: UpdateStatusRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.status, *status);
        }
    }

    #[test]
    fn valid_status_values_list_is_complete() {
        let valid_statuses = ["draft", "running", "paused", "completed", "failed"];
        assert!(valid_statuses.contains(&"draft"));
        assert!(valid_statuses.contains(&"running"));
        assert!(valid_statuses.contains(&"paused"));
        assert!(valid_statuses.contains(&"completed"));
        assert!(valid_statuses.contains(&"failed"));
        assert!(!valid_statuses.contains(&"deleted"));
    }
}
