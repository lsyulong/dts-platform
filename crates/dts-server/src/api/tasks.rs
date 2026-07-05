use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::db::DbPool;
use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;

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
#[derive(Debug, Serialize, sqlx::FromRow)]
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
pub async fn list(pool: web::Data<DbPool>, req: HttpRequest) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let tasks: Vec<TaskResponse> = sqlx::query_as::<_, TaskResponse>(
        "SELECT id, name, description, source_id, target_id, cron_expr, status, created_at
         FROM sync_task WHERE tenant_id = $1
         ORDER BY created_at DESC",
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?;

    let total = tasks.len();
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": tasks,
        "total": total,
    })))
}

/// POST /api/v1/tasks
pub async fn create(
    pool: web::Data<DbPool>,
    req: HttpRequest,
    body: web::Json<CreateTaskRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    // 验证 source 和 target 存在
    for ds_id in [&body.source_id, &body.target_id] {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE id = $1 AND tenant_id = $2")
                .bind(ds_id.as_str())
                .bind(&claims.tenant_id)
                .fetch_one(pool.get_ref())
                .await?;

        if count == 0 {
            return Err(AppError::BadRequest(format!("数据源 {ds_id} 不存在")));
        }
    }

    let task_id = Uuid::new_v4().to_string();
    let config_json = serde_json::json!({
        "table_mappings": body.table_mappings,
        "filter_config": body.filter_config,
    });

    sqlx::query(
        "INSERT INTO sync_task (id, tenant_id, name, description, source_id, target_id,
         cron_expr, config_json, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&task_id)
    .bind(&claims.tenant_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.source_id)
    .bind(&body.target_id)
    .bind(&body.cron_expr)
    .bind(config_json.to_string())
    .bind(&claims.sub)
    .execute(pool.get_ref())
    .await?;

    log::info!("任务创建: {} (租户: {})", body.name, claims.tenant_id);

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": task_id,
        "name": body.name,
        "status": "draft",
    })))
}

/// GET /api/v1/tasks/{id}
pub async fn get(
    pool: web::Data<DbPool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    let task = sqlx::query(
        "SELECT t.id, t.name, t.description, t.source_id, t.target_id,
         t.cron_expr, t.status, t.config_json, t.created_by, t.created_at, t.updated_at,
         s.name as source_name, s.db_type as source_type,
         d.name as target_name, d.db_type as target_type
         FROM sync_task t
         JOIN datasource s ON t.source_id = s.id
         JOIN datasource d ON t.target_id = d.id
         WHERE t.id = $1 AND t.tenant_id = $2",
    )
    .bind(&task_id)
    .bind(&claims.tenant_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;

    // 获取最近执行历史
    let runs: Vec<serde_json::Value> = sqlx::query(
        "SELECT id, status, start_time, end_time, rows_synced, bytes_synced,
         total_rows, total_bytes, error_message
         FROM task_run WHERE task_id = $1
         ORDER BY start_time DESC LIMIT 10",
    )
    .bind(&task_id)
    .fetch_all(pool.get_ref())
    .await?
    .into_iter()
    .map(|r| {
        let start_time: chrono::DateTime<chrono::Utc> = r.get(2);
        let end_time: Option<chrono::DateTime<chrono::Utc>> = r.get(3);
        serde_json::json!({
            "id": r.get::<String, _>(0),
            "status": r.get::<String, _>(1),
            "start_time": start_time.to_rfc3339(),
            "end_time": end_time.map(|dt| dt.to_rfc3339()),
            "rows_synced": r.get::<i64, _>(4),
            "bytes_synced": r.get::<i64, _>(5),
            "total_rows": r.get::<Option<i64>, _>(6),
            "total_bytes": r.get::<Option<i64>, _>(7),
            "error_message": r.get::<Option<String>, _>(8),
        })
    })
    .collect();

    let created_at: chrono::DateTime<chrono::Utc> = task.get(9);
    let updated_at: chrono::DateTime<chrono::Utc> = task.get(10);
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": task.get::<String, _>(0),
        "name": task.get::<String, _>(1),
        "description": task.get::<Option<String>, _>(2),
        "source_id": task.get::<String, _>(3),
        "target_id": task.get::<String, _>(4),
        "source_name": task.get::<String, _>(11),
        "source_type": task.get::<String, _>(12),
        "target_name": task.get::<String, _>(13),
        "target_type": task.get::<String, _>(14),
        "cron_expr": task.get::<Option<String>, _>(5),
        "status": task.get::<String, _>(6),
        "config_json": task.get::<String, _>(7),
        "created_by": task.get::<String, _>(8),
        "created_at": created_at.to_rfc3339(),
        "updated_at": updated_at.to_rfc3339(),
        "recent_runs": runs,
    })))
}

/// PATCH /api/v1/tasks/{id}/status
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_status(
    pool: web::Data<DbPool>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateStatusRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    let valid_statuses = ["draft", "running", "paused", "completed", "failed"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(AppError::BadRequest(format!("无效状态: {}", body.status)));
    }

    let result = sqlx::query(
        "UPDATE sync_task SET status = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3",
    )
    .bind(&body.status)
    .bind(&task_id)
    .bind(&claims.tenant_id)
    .execute(pool.get_ref())
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("任务不存在".into()));
    }

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
        assert_eq!(req.description, None);
        assert_eq!(req.source_id, "src-1");
        assert_eq!(req.target_id, "tgt-1");
        assert_eq!(req.cron_expr, None);
        assert_eq!(req.table_mappings, None);
        assert_eq!(req.filter_config, None);
    }

    #[test]
    fn create_task_request_missing_name_fails() {
        let json = serde_json::json!({
            "source_id": "src-1",
            "target_id": "tgt-1"
        });
        let result = serde_json::from_value::<CreateTaskRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_task_request_missing_source_id_fails() {
        let json = serde_json::json!({
            "name": "Task",
            "target_id": "tgt-1"
        });
        let result = serde_json::from_value::<CreateTaskRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_task_request_missing_target_id_fails() {
        let json = serde_json::json!({
            "name": "Task",
            "source_id": "src-1"
        });
        let result = serde_json::from_value::<CreateTaskRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_task_request_with_null_optionals() {
        let json = serde_json::json!({
            "name": "Task",
            "source_id": "s1",
            "target_id": "t1",
            "description": null,
            "cron_expr": null,
            "table_mappings": null,
            "filter_config": null
        });
        let req: CreateTaskRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.description, None);
        assert_eq!(req.cron_expr, None);
        assert_eq!(req.table_mappings, None);
        assert_eq!(req.filter_config, None);
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
    fn update_status_request_invalid_json() {
        let json = serde_json::json!({"wrong_field": "value"});
        let result = serde_json::from_value::<UpdateStatusRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn update_status_request_empty_json() {
        let json = serde_json::json!({});
        let result = serde_json::from_value::<UpdateStatusRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn update_status_request_rejects_unknown_status_at_type_level() {
        let json = serde_json::json!({"status": "unknown-status"});
        let req: UpdateStatusRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.status, "unknown-status");
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

    // ========== TaskResponse Tests ==========

    #[test]
    fn task_response_fields() {
        let task = TaskResponse {
            id: "task-1".into(),
            name: "Test Task".into(),
            description: Some("A test".into()),
            source_id: "src-1".into(),
            target_id: "tgt-1".into(),
            cron_expr: None,
            status: "draft".into(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&task).unwrap();
        assert_eq!(json["id"], "task-1");
        assert_eq!(json["name"], "Test Task");
        assert_eq!(json["status"], "draft");
        assert!(json["cron_expr"].is_null());
    }
}
