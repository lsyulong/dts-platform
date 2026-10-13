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

use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};

use crate::middleware::auth;
use crate::models::error::AppError;
use crate::services::task_svc::{PipelineConfig, TaskService, default_batch_size};
use dts_backend::repo::TaskRepo;
use dts_engine::{SyncMode, TableMapping};

/// 创建任务请求
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskRequest {
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub table_mappings: Option<Vec<TableMapping>>,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
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
    let claims = auth::require_permission(&req, "task:read").await?;

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
    let claims = auth::require_permission(&req, "task:write").await?;

    let config = PipelineConfig {
        table_mappings: body.table_mappings.clone().unwrap_or_default(),
        sync_mode: body.sync_mode.clone(),
        batch_size: body.batch_size,
        filter_config: body.filter_config.clone(),
    };
    config.validate().map_err(AppError::from)?;
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("任务名称不能为空".into()));
    }
    let config_json = serde_json::to_string(&config)
        .map_err(|e| AppError::Internal(format!("序列化任务配置失败: {e}")))?;

    let task_id = task_svc
        .create(
            &claims.tenant_id,
            &body.name,
            body.description.as_deref(),
            &body.source_id,
            &body.target_id,
            body.cron_expr.as_deref(),
            &config_json,
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
    let claims = auth::require_permission(&req, "task:read").await?;
    let task_id = path.into_inner();

    let task = task_repo
        .find_detail_by_id(&task_id, &claims.tenant_id)
        .await?
        .ok_or_else(|| AppError::NotFound("任务不存在".into()))?;

    Ok(HttpResponse::Ok().json(task))
}

/// POST /api/v1/tasks/{id}/start
pub async fn start(
    scheduler: web::Data<crate::services::scheduler::TaskScheduler>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "task:execute").await?;
    let task_id = path.into_inner();

    let run_id = scheduler.start_task(&task_id, &claims.tenant_id).await?;

    tracing::info!(
        task_id = %task_id,
        run_id = %run_id,
        tenant_id = %claims.tenant_id,
        "任务启动成功"
    );

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "run_id": run_id,
        "status": "running",
        "message": "任务已提交到引擎",
    })))
}

/// PATCH /api/v1/tasks/{id}/status
#[derive(Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub async fn update_status(
    scheduler: web::Data<crate::services::scheduler::TaskScheduler>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<UpdateStatusRequest>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "task:execute").await?;
    let task_id = path.into_inner();

    if body.status != "cancelled" {
        return Err(AppError::BadRequest(
            "运行状态由引擎报告；当前只接受 cancelled 取消请求，不支持暂停".into(),
        ));
    }
    scheduler.cancel_task(&task_id, &claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": task_id,
        "status": "cancelling",
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
            "table_mappings": [{"source_table": "users", "target_table": "users_backup"}],
            "sync_mode": "full", "batch_size": 500,
            "filter_config": null
        });
        let req: CreateTaskRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "My Sync Task");
        assert_eq!(req.description, Some("Syncs user data".into()));
        assert_eq!(req.source_id, "src-1");
        assert_eq!(req.target_id, "tgt-1");
        assert_eq!(req.cron_expr, Some("0 */6 * * *".into()));
        assert!(req.table_mappings.is_some());
        assert_eq!(req.sync_mode, SyncMode::Full);
        assert_eq!(req.batch_size, 500);
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

    #[test]
    fn legacy_mapping_object_is_rejected_instead_of_silently_syncing_every_table() {
        let json = serde_json::json!({
            "name": "Task", "source_id": "src", "target_id": "dst",
            "table_mappings": {"users": "users_backup"}
        });
        assert!(serde_json::from_value::<CreateTaskRequest>(json).is_err());
    }

    #[test]
    fn unsupported_mode_is_rejected_without_a_snapshot_fallback() {
        let request: CreateTaskRequest = serde_json::from_value(serde_json::json!({
            "name":"Task", "source_id":"src", "target_id":"dst", "sync_mode":"incremental",
            "table_mappings":[{"source_table":"orders","target_table":"fact_orders"}]
        }))
        .unwrap();
        assert!(
            PipelineConfig {
                table_mappings: request.table_mappings.unwrap(),
                sync_mode: request.sync_mode,
                batch_size: request.batch_size,
                filter_config: request.filter_config
            }
            .validate()
            .is_err()
        );
    }
}
