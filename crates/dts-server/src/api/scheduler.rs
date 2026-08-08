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
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::config::SchedulerConfig;
use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;

/// 定时调度请求
#[derive(Debug, Deserialize)]
pub struct SetScheduleRequest {
    pub cron_expr: String,
}

/// 调度状态响应
#[derive(Debug, Serialize)]
pub struct ScheduleStatus {
    pub has_timer: bool,
    pub cron_expr: Option<String>,
    pub next_run: Option<String>,
    pub timer_name: Option<String>,
    pub schedule_history: Vec<ScheduleInstance>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleInstance {
    pub id: u64,
    pub name: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub exit_code: Option<i32>,
    pub status: String,
}

/// POST /api/v1/tasks/{id}/schedule — 设置定时调度
pub async fn set_schedule(
    scheduler_config: web::Data<SchedulerConfig>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetScheduleRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    if !scheduler_config.enabled {
        return Err(AppError::BadRequest("jiascheduler 集成未启用".into()));
    }

    let client = dts_scheduler_client::JiaSchedulerClient::login(
        &scheduler_config.base_url,
        &scheduler_config.username,
        &scheduler_config.password,
    )
    .await
    .map_err(|e| {
        error!(error = %e, "jiascheduler 登录失败");
        AppError::Internal(format!("调度服务连接失败: {e}"))
    })?;

    let sync = dts_scheduler_client::TaskSync::new(client)
        .await
        .map_err(|e| AppError::Internal(format!("调度初始化失败: {e}")))?;

    // 同步任务到 jiascheduler（包含 cron 定时器）
    let eid = sync
        .upsert_job(
            &task_id,
            &task_id, // 任务名称从 DB 查，这里简化用 ID
            &scheduler_config.callback_base_url,
            &scheduler_config.service_token,
            Some(&body.cron_expr),
        )
        .await
        .map_err(|e| AppError::Internal(format!("同步到调度服务失败: {e}")))?;

    info!(task_id = %task_id, eid = %eid, cron = %body.cron_expr, "定时调度已设置");

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "task_id": task_id,
        "eid": eid,
        "cron_expr": body.cron_expr,
        "status": "scheduled",
    })))
}

/// DELETE /api/v1/tasks/{id}/schedule — 取消定时调度
pub async fn delete_schedule(
    scheduler_config: web::Data<SchedulerConfig>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    if !scheduler_config.enabled {
        return Err(AppError::BadRequest("jiascheduler 集成未启用".into()));
    }

    let client = dts_scheduler_client::JiaSchedulerClient::login(
        &scheduler_config.base_url,
        &scheduler_config.username,
        &scheduler_config.password,
    )
    .await
    .map_err(|e| AppError::Internal(format!("调度服务连接失败: {e}")))?;

    let sync = dts_scheduler_client::TaskSync::new(client)
        .await
        .map_err(|e| AppError::Internal(format!("调度初始化失败: {e}")))?;

    let eid = dts_scheduler_client::TaskSync::task_id_to_eid(&task_id);
    sync.remove_timer(&eid)
        .await
        .map_err(|e| AppError::Internal(format!("取消调度失败: {e}")))?;

    info!(task_id = %task_id, "定时调度已取消");

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "task_id": task_id,
        "status": "unscheduled",
    })))
}

/// GET /api/v1/tasks/{id}/schedule — 查询调度状态
pub async fn get_schedule(
    scheduler_config: web::Data<SchedulerConfig>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;
    let task_id = path.into_inner();

    if !scheduler_config.enabled {
        return Ok(HttpResponse::Ok().json(ScheduleStatus {
            has_timer: false,
            cron_expr: None,
            next_run: None,
            timer_name: None,
            schedule_history: vec![],
        }));
    }

    let client = match dts_scheduler_client::JiaSchedulerClient::login(
        &scheduler_config.base_url,
        &scheduler_config.username,
        &scheduler_config.password,
    )
    .await
    {
        Ok(c) => c,
        Err(_) => {
            return Ok(HttpResponse::Ok().json(ScheduleStatus {
                has_timer: false,
                cron_expr: None,
                next_run: None,
                timer_name: None,
                schedule_history: vec![],
            }));
        },
    };

    let eid = dts_scheduler_client::TaskSync::task_id_to_eid(&task_id);

    // 查询定时器
    let timers = client
        .query_timer(&dts_scheduler_client::models::QueryTimerParams {
            name: Some(format!("DTS-Timer-{eid}")),
            job_type: None,
            page: 1,
            page_size: 5,
        })
        .await
        .unwrap_or(dts_scheduler_client::models::QueryJobTimerResp {
            data: vec![],
            total: 0,
        });

    // 查询调度历史
    let history = client.query_schedule_history(&eid).await.unwrap_or(
        dts_scheduler_client::models::QueryScheduleHistoryResp {
            data: vec![],
            total: 0,
        },
    );

    let has_timer = !timers.data.is_empty();
    let first_timer = timers.data.first();

    Ok(HttpResponse::Ok().json(ScheduleStatus {
        has_timer,
        cron_expr: first_timer.and_then(|t| {
            t.timer_expr
                .as_ref()
                .and_then(|v| v.get("cron").and_then(|c| c.as_str().map(String::from)))
        }),
        next_run: None, // jiascheduler 不直接提供 next_run
        timer_name: first_timer.map(|t| t.name.clone()),
        schedule_history: history
            .data
            .into_iter()
            .map(|h| ScheduleInstance {
                status: if h.exit_code.is_none_or(|c| c == 0) {
                    "success".into()
                } else {
                    "failed".into()
                },
                id: h.id,
                name: h.name,
                start_time: h.start_time,
                end_time: h.end_time,
                exit_code: h.exit_code,
            })
            .collect(),
    }))
}
