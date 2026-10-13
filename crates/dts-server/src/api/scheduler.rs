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
use crate::models::error::AppError;

/// 定时调度请求
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetScheduleRequest {
    pub cron_expr: String,
}

/// 调度状态响应
#[derive(Debug, Serialize)]
pub struct ScheduleStatus {
    pub enabled: bool,
    pub has_timer: bool,
    pub active: bool,
    pub state: String,
    pub timezone: String,
    pub cron_expr: Option<String>,
    pub next_run: Option<String>,
    pub timer_name: Option<String>,
    pub schedule_history: Vec<ScheduleInstance>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleInstance {
    pub id: u64,
    pub name: String,
    pub created_time: String,
    pub status: String,
}

/// POST /api/v1/tasks/{id}/schedule — 设置定时调度
pub async fn set_schedule(
    scheduler_config: web::Data<SchedulerConfig>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetScheduleRequest>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "task:execute").await?;
    let task_id = path.into_inner();
    let task = tenant_task(&req, &task_id, &claims.tenant_id).await?;

    if !scheduler_config.enabled {
        return Err(AppError::BadRequest("jiascheduler 集成未启用".into()));
    }
    super::scheduler_trigger::validate_service_config(&scheduler_config)?;
    dts_scheduler_client::models::TimerExpr::from_cron(&body.cron_expr)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    scheduler_config
        .validate_instance()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let grant = super::scheduler_trigger::trigger_token(
        &scheduler_config.service_token,
        &claims.tenant_id,
        &task_id,
        &claims.sub,
    );
    let start_url = scheduler_config
        .trigger_url(&task_id, &claims.tenant_id, &claims.sub)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

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

    let sync = dts_scheduler_client::TaskSync::new(client, &scheduler_config.instance_id)
        .await
        .map_err(|e| AppError::Internal(format!("调度初始化失败: {e}")))?;

    // 同步任务到 jiascheduler（包含 cron 定时器）
    let eid = sync
        .upsert_job(
            &task_id,
            &task.name,
            &start_url,
            &grant,
            Some(&body.cron_expr),
        )
        .await
        .map_err(|e| AppError::Internal(format!("同步到调度服务失败: {e}")))?;

    info!(task_id = %task_id, eid = %eid, cron = %body.cron_expr, "定时调度已设置");

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "task_id": task_id,
        "eid": eid,
        "cron_expr": body.cron_expr,
        "status": "dispatched",
        "timezone": "utc",
    })))
}

/// DELETE /api/v1/tasks/{id}/schedule — 取消定时调度
pub async fn delete_schedule(
    scheduler_config: web::Data<SchedulerConfig>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "task:execute").await?;
    let task_id = path.into_inner();
    let _task = tenant_task(&req, &task_id, &claims.tenant_id).await?;

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

    dts_scheduler_client::TaskSync::remove_task_timer(&client, &task_id)
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
    let claims = auth::require_permission(&req, "task:read").await?;
    let task_id = path.into_inner();
    let _task = tenant_task(&req, &task_id, &claims.tenant_id).await?;

    if !scheduler_config.enabled {
        return Ok(HttpResponse::Ok().json(ScheduleStatus {
            enabled: false,
            has_timer: false,
            active: false,
            state: "disabled".into(),
            timezone: "utc".into(),
            cron_expr: None,
            next_run: None,
            timer_name: None,
            schedule_history: vec![],
        }));
    }

    let client = dts_scheduler_client::JiaSchedulerClient::login(
        &scheduler_config.base_url,
        &scheduler_config.username,
        &scheduler_config.password,
    )
    .await
    .map_err(|_| AppError::Internal("调度服务连接失败".into()))?;

    Ok(HttpResponse::Ok().json(read_schedule_status(&client, &task_id).await?))
}

async fn read_schedule_status(
    client: &dts_scheduler_client::JiaSchedulerClient,
    task_id: &str,
) -> Result<ScheduleStatus, AppError> {
    let timer_name = dts_scheduler_client::TaskSync::timer_name(task_id)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let timer = client
        .find_timer(&timer_name)
        .await
        .map_err(|e| AppError::Internal(format!("读取调度状态失败: {e}")))?;
    let job_name = dts_scheduler_client::TaskSync::job_name(task_id)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let job = client
        .find_job(&job_name)
        .await
        .map_err(|e| AppError::Internal(format!("读取调度作业失败: {e}")))?;
    let mut runtime = None;
    let mut schedule_history = Vec::new();
    if let Some(job) = &job {
        if timer.as_ref().is_some_and(|timer| job.eid != timer.eid) {
            return Err(AppError::Internal(
                "定时器关联的作业与当前任务不一致".into(),
            ));
        }
        runtime = client
            .find_schedule(&job.eid, &timer_name)
            .await
            .map_err(|e| AppError::Internal(format!("读取实际调度失败: {e}")))?;
        schedule_history = client
            .query_schedule_history(&job.eid, &timer_name)
            .await
            .map_err(|e| AppError::Internal(format!("读取调度历史失败: {e}")))?
            .list
            .into_iter()
            .map(|record| ScheduleInstance {
                id: record.id,
                name: record.name,
                created_time: record.created_time,
                status: "triggered".into(),
            })
            .collect();
    } else if timer.is_some() {
        return Err(AppError::Internal("定时器对应的调度作业不存在".into()));
    }
    let (active, state) = runtime_state(client, runtime.as_ref()).await?;
    let cron_expr = schedule_cron(runtime.as_ref(), timer.as_ref())?;
    Ok(ScheduleStatus {
        enabled: true,
        has_timer: timer.is_some(),
        active,
        state,
        timezone: "utc".into(),
        cron_expr,
        next_run: None,
        timer_name: timer.map(|timer| timer.name),
        schedule_history,
    })
}

async fn runtime_state(
    client: &dts_scheduler_client::JiaSchedulerClient,
    schedule: Option<&dts_scheduler_client::models::ScheduleRecord>,
) -> Result<(bool, String), AppError> {
    let Some(schedule) = schedule else {
        return Ok((false, "inactive".into()));
    };
    if schedule.action == "stop_timer" {
        return Ok((false, "stopped".into()));
    }
    if schedule.action != "start_timer" || schedule.instance_ids.len() != 1 {
        return Err(AppError::Internal("调度记录不是预期的单节点 timer".into()));
    }
    let runs = client
        .running_status(&schedule.eid, &schedule.name)
        .await
        .map_err(|e| AppError::Internal(format!("读取 worker 调度状态失败: {e}")))?;
    Ok(state_from_runs(schedule, &runs))
}

fn state_from_runs(
    schedule: &dts_scheduler_client::models::ScheduleRecord,
    runs: &[dts_scheduler_client::models::RunningStatusRecord],
) -> (bool, String) {
    let runs: Vec<_> = runs
        .iter()
        .filter(|run| schedule.instance_ids.contains(&run.instance_id))
        .collect();
    if runs
        .iter()
        .any(|run| run.is_online && run.schedule_status == "scheduling")
    {
        return (true, "active".into());
    }
    if runs.iter().any(|run| !run.is_online) {
        return (false, "offline".into());
    }
    (false, "pending".into())
}

fn schedule_cron(
    runtime: Option<&dts_scheduler_client::models::ScheduleRecord>,
    timer: Option<&dts_scheduler_client::models::JobTimerRecord>,
) -> Result<Option<String>, AppError> {
    if let Some(expr) = runtime.and_then(|schedule| schedule.timer_expr.as_ref()) {
        if expr.timezone != "utc" {
            return Err(AppError::Internal("请重新设置调度以使用 UTC 时区".into()));
        }
        return dts_scheduler_client::models::TimerExpr::from_cron(&expr.expr)
            .map(|expr| Some(expr.to_cron()))
            .map_err(|e| AppError::Internal(e.to_string()));
    }
    if let Some(expr) = timer.and_then(|timer| timer.timer_expr.as_ref()) {
        if expr.timezone != "utc" {
            return Err(AppError::Internal("请重新设置调度以使用 UTC 时区".into()));
        }
        return Ok(Some(expr.to_cron()));
    }
    Ok(None)
}

async fn tenant_task(
    req: &HttpRequest,
    id: &str,
    tenant_id: &str,
) -> Result<dts_backend::types::TaskDetail, AppError> {
    let repo = req
        .app_data::<web::Data<std::sync::Arc<dyn dts_backend::TaskRepo>>>()
        .ok_or_else(|| AppError::Internal("任务存储未配置".into()))?;
    repo.find_detail_by_id(id, tenant_id)
        .await?
        .ok_or_else(|| AppError::NotFound("任务不存在".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_schedule_requires_confirmed_online_worker_state() {
        let schedule = serde_json::from_value(serde_json::json!({
            "id": 88, "eid": "real-eid", "name": "timer", "job_type": "default",
            "schedule_type": "timer", "action": "start_timer", "instance_ids": ["worker-1"], "timer_expr": null,
        })).unwrap();
        let mut run = serde_json::from_value::<dts_scheduler_client::models::RunningStatusRecord>(
            serde_json::json!({
                "eid": "real-eid", "instance_id": "worker-1", "is_online": true,
                "schedule_name": "timer", "schedule_status": "scheduling", "next_time": "",
            }),
        )
        .unwrap();
        assert_eq!(state_from_runs(&schedule, &[]), (false, "pending".into()));
        assert_eq!(
            state_from_runs(&schedule, std::slice::from_ref(&run)),
            (true, "active".into())
        );
        run.is_online = false;
        assert_eq!(
            state_from_runs(&schedule, std::slice::from_ref(&run)),
            (false, "offline".into())
        );
        run.instance_id = "foreign-worker".into();
        assert_eq!(
            state_from_runs(&schedule, &[run]),
            (false, "pending".into())
        );
    }
}
