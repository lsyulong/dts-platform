use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::middleware::auth;
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
    pub created_at: String,
}

/// GET /api/v1/tasks
pub async fn list(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let tasks: Vec<TaskResponse> = sqlx::query_as::<_, TaskResponse>(
        "SELECT id, name, description, source_id, target_id, cron_expr, status, created_at
         FROM sync_task WHERE tenant_id = ?
         ORDER BY created_at DESC"
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
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    body: web::Json<CreateTaskRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    // 验证 source 和 target 存在
    for ds_id in [&body.source_id, &body.target_id] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM datasource WHERE id = ? AND tenant_id = ?"
        )
        .bind(ds_id.as_str())
        .bind(&claims.tenant_id)
        .fetch_one(pool.get_ref())
        .await?;

        if count == 0 {
            return Err(AppError::BadRequest(format!("数据源 {} 不存在", ds_id)));
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
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&task_id)
    .bind(&claims.tenant_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.source_id)
    .bind(&body.target_id)
    .bind(&body.cron_expr)
    .bind(&config_json.to_string())
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
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
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
         WHERE t.id = ? AND t.tenant_id = ?"
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
         FROM task_run WHERE task_id = ?
         ORDER BY start_time DESC LIMIT 10"
    )
    .bind(&task_id)
    .fetch_all(pool.get_ref())
    .await?
    .into_iter()
    .map(|r| {
        serde_json::json!({
            "id": r.get::<String, _>(0),
            "status": r.get::<String, _>(1),
            "start_time": r.get::<String, _>(2),
            "end_time": r.get::<Option<String>, _>(3),
            "rows_synced": r.get::<i64, _>(4),
            "bytes_synced": r.get::<i64, _>(5),
            "total_rows": r.get::<Option<i64>, _>(6),
            "total_bytes": r.get::<Option<i64>, _>(7),
            "error_message": r.get::<Option<String>, _>(8),
        })
    })
    .collect();

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
        "created_at": task.get::<String, _>(9),
        "updated_at": task.get::<String, _>(10),
        "recent_runs": runs,
    })))
}

/// PATCH /api/v1/tasks/{id}/status
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_status(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateStatusRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(&req);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    let valid_statuses = ["draft", "running", "paused", "completed", "failed"];
    if !valid_statuses.contains(&body.status.as_str()) {
        return Err(AppError::BadRequest(format!("无效状态: {}", body.status)));
    }

    let result = sqlx::query(
        "UPDATE sync_task SET status = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?"
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

fn get_jwt_secret(_req: &HttpRequest) -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string())
}
