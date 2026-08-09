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
use dts_backend::repo::{DatasourceRepo, TaskRepo};
use dts_backend::repo::task_run::TaskRunRepo;
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::middleware::auth;
use crate::models::error::AppError;
use crate::services::ai_svc::DiagnosisContext;

#[derive(Deserialize)]
pub struct AiChatQuery {
    pub message: String,
    #[serde(default)]
    pub route: Option<String>,
}

#[derive(Deserialize)]
pub struct AiDiagnoseQuery {
    pub task_id: String,
}

/// POST /api/v1/ai/chat — SSE 流式 AI 对话
pub async fn chat(
    ai_svc: web::Data<crate::services::ai_svc::AiService>,
    req: HttpRequest,
    query: web::Query<AiChatQuery>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = auth::get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;

    let route = query.route.clone().unwrap_or_default();
    let rx = ai_svc
        .chat_stream(query.message.clone(), route, vec![])
        .await?;

    Ok(sse_response(rx))
}

/// POST /api/v1/ai/diagnose — SSE 流式任务故障诊断
pub async fn diagnose(
    ai_svc: web::Data<crate::services::ai_svc::AiService>,
    task_repo: web::Data<Arc<dyn TaskRepo>>,
    task_run_repo: web::Data<Arc<dyn TaskRunRepo>>,
    datasource_repo: web::Data<Arc<dyn DatasourceRepo>>,
    req: HttpRequest,
    query: web::Query<AiDiagnoseQuery>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = auth::get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    // 1. 获取任务详情
    let task = task_repo
        .find_detail_by_id(&query.task_id, &claims.tenant_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("无法获取任务信息: {e}")))?
        .ok_or_else(|| AppError::BadRequest("任务不存在".into()))?;

    // 2. 获取数据源类型
    let source = datasource_repo
        .find_detail_by_id(&task.source_id, &claims.tenant_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("无法获取源端信息: {e}")))?;
    let target = datasource_repo
        .find_detail_by_id(&task.target_id, &claims.tenant_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("无法获取目标端信息: {e}")))?;

    // 3. 获取最近运行历史
    let recent_runs = task_run_repo
        .find_recent(&query.task_id, 10)
        .await
        .map_err(|e| AppError::BadRequest(format!("无法获取运行历史: {e}")))?;

    // 4. 查找最近的错误信息
    let latest_error = recent_runs
        .iter()
        .find(|r| r.error_message.is_some())
        .and_then(|r| r.error_message.clone());

    let latest_run = recent_runs.first();

    // 5. 构建诊断上下文
    let ctx = DiagnosisContext {
        task_name: task.name.clone(),
        task_description: task.description.clone(),
        source_db_type: source.map(|s| s.db_type).unwrap_or_else(|| "unknown".into()),
        target_db_type: target.map(|t| t.db_type).unwrap_or_else(|| "unknown".into()),
        run_status: latest_run
            .map(|r| r.status.clone())
            .unwrap_or_else(|| "unknown".into()),
        error_message: latest_error,
        rows_synced: latest_run.map(|r| r.rows_synced).unwrap_or(0),
        recent_history: recent_runs
            .iter()
            .map(|r| crate::services::ai_svc::RunHistoryEntry {
                status: r.status.clone(),
                rows_synced: r.rows_synced,
                error_message: r.error_message.clone(),
                start_time: r.start_time.clone(),
            })
            .collect(),
    };

    let rx = ai_svc.diagnose(ctx).await?;
    Ok(sse_response(rx))
}

/// 将 channel receiver 转为 actix SSE 流式响应
fn sse_response(rx: tokio::sync::mpsc::Receiver<String>) -> HttpResponse {
    let receiver_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let sse_stream = receiver_stream.map(|text| {
        let data = format!("data: {}\n\n", text.replace('\n', "\\n"));
        Ok::<_, actix_web::Error>(web::Bytes::from(data))
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(sse_stream)
}
