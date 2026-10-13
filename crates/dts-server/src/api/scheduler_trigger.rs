use actix_web::{HttpRequest, HttpResponse, web};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::hmac;
use serde::Deserialize;

use crate::config::SchedulerConfig;
use crate::models::{auth::AuthIdentity, error::AppError};
use crate::services::{auth_svc::AuthService, scheduler::TaskScheduler};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerScope {
    pub tenant_id: String,
    pub actor_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowTriggerScope {
    pub tenant_id: String,
    pub actor_id: String,
    pub plan_version: u64,
}

fn scope_message(tenant: &str, task: &str, actor: &str) -> Vec<u8> {
    let mut message = b"dts.scheduler.trigger.v1".to_vec();
    for value in [tenant, task, actor] {
        message.extend_from_slice(&(value.len() as u64).to_be_bytes());
        message.extend_from_slice(value.as_bytes());
    }
    message
}

pub(crate) fn trigger_token(secret: &str, tenant: &str, task: &str, actor: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    URL_SAFE_NO_PAD.encode(hmac::sign(&key, &scope_message(tenant, task, actor)).as_ref())
}

fn workflow_scope_message(tenant: &str, workflow: &str, actor: &str, plan_version: u64) -> Vec<u8> {
    let mut message = b"dts.scheduler.workflow-trigger.v1".to_vec();
    for value in [tenant, workflow, actor] {
        message.extend_from_slice(&(value.len() as u64).to_be_bytes());
        message.extend_from_slice(value.as_bytes());
    }
    message.extend_from_slice(&plan_version.to_be_bytes());
    message
}

pub(crate) fn workflow_trigger_token(
    secret: &str,
    tenant: &str,
    workflow: &str,
    actor: &str,
    plan_version: u64,
) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    URL_SAFE_NO_PAD.encode(
        hmac::sign(
            &key,
            &workflow_scope_message(tenant, workflow, actor, plan_version),
        )
        .as_ref(),
    )
}

pub(crate) fn validate_service_config(config: &SchedulerConfig) -> Result<(), AppError> {
    if !config.enabled {
        return Err(AppError::BadRequest("jiascheduler 集成未启用".into()));
    }
    if config.service_token.len() < 32 || config.service_token == "dev-service-token-change-me" {
        return Err(AppError::BadRequest(
            "请配置至少 32 字节的 JIA_SERVICE_TOKEN".into(),
        ));
    }
    Ok(())
}

/// A scheduler grant is scoped to one task, tenant and authorizing user.
/// The shared signing secret is never sent to scheduler agents.
pub async fn start(
    req: HttpRequest,
    path: web::Path<String>,
    scope: web::Query<TriggerScope>,
    config: web::Data<SchedulerConfig>,
    auth: web::Data<AuthService>,
    scheduler: web::Data<TaskScheduler>,
) -> Result<HttpResponse, AppError> {
    validate_service_config(&config)?;
    let task_id = path.into_inner();
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(|| AppError::Unauthorized("缺少有效的调度授权".into()))?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, config.service_token.as_bytes());
    hmac::verify(
        &key,
        &scope_message(&scope.tenant_id, &task_id, &scope.actor_id),
        &token,
    )
    .map_err(|_| AppError::Unauthorized("调度授权无效".into()))?;
    let user = auth
        .user_repo
        .find_detail_by_id(&scope.actor_id)
        .await?
        .filter(|user| user.tenant_id == scope.tenant_id)
        .ok_or_else(|| AppError::Unauthorized("调度授权用户不存在".into()))?;
    let claims = AuthIdentity {
        sub: user.id,
        tenant_id: scope.tenant_id.clone(),
        username: user.username,
    };
    auth.validate_identity(&claims).await?;
    auth.authorize(&claims, "task:execute").await?;
    let run_id = scheduler.start_task(&task_id, &claims.tenant_id).await?;
    Ok(HttpResponse::Accepted().json(serde_json::json!({"run_id":run_id,"status":"running"})))
}

pub async fn start_workflow(
    req: HttpRequest,
    path: web::Path<String>,
    scope: web::Query<WorkflowTriggerScope>,
    config: web::Data<SchedulerConfig>,
    auth: web::Data<AuthService>,
    schedules: web::Data<crate::services::development_schedules::DevelopmentScheduleService>,
    jobs: web::Data<crate::services::product_jobs::ProductJobService>,
) -> Result<HttpResponse, AppError> {
    validate_service_config(&config)?;
    let workflow_id = path.into_inner();
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(|| AppError::Unauthorized("缺少有效的工作流调度授权".into()))?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, config.service_token.as_bytes());
    hmac::verify(
        &key,
        &workflow_scope_message(
            &scope.tenant_id,
            &workflow_id,
            &scope.actor_id,
            scope.plan_version,
        ),
        &token,
    )
    .map_err(|_| AppError::Unauthorized("工作流调度授权无效".into()))?;
    let plan = schedules
        .trigger_context(
            &scope.tenant_id,
            &workflow_id,
            &scope.actor_id,
            scope.plan_version,
        )
        .await?;
    let user = auth
        .user_repo
        .find_detail_by_id(&scope.actor_id)
        .await?
        .filter(|user| user.tenant_id == scope.tenant_id)
        .ok_or_else(|| AppError::Unauthorized("周期计划授权用户不存在".into()))?;
    let actor = AuthIdentity {
        sub: user.id,
        tenant_id: scope.tenant_id.clone(),
        username: user.username,
    };
    auth.validate_identity(&actor).await?;
    auth.authorize(&actor, "development:execute").await?;
    let idempotency_key = format!(
        "scheduled:{}:{}",
        plan.plan_version,
        chrono::Utc::now().timestamp()
    );
    let enqueued = jobs
        .enqueue(
            &actor,
            "development_workflow",
            &workflow_id,
            crate::services::product_jobs::RunRequest {
                expected_version: plan.source_version,
                idempotency_key,
                parameters: serde_json::json!({}),
            },
        )
        .await?;
    let view = jobs.submission_view(&actor, enqueued).await?;
    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "run_id": view.job.id,
        "created": view.created,
        "status": view.job.status,
        "plan_version": plan.plan_version,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_grants_are_scoped_to_task_tenant_and_actor() {
        let secret = "synthetic-test-service-signing-secret";
        let token = trigger_token(secret, "tenant", "task", "actor");
        let decoded = URL_SAFE_NO_PAD.decode(token).unwrap();
        let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
        assert!(hmac::verify(&key, &scope_message("tenant", "task", "actor"), &decoded).is_ok());
        for scope in [
            ("other", "task", "actor"),
            ("tenant", "other", "actor"),
            ("tenant", "task", "other"),
        ] {
            assert!(
                hmac::verify(&key, &scope_message(scope.0, scope.1, scope.2), &decoded).is_err()
            );
        }
    }

    #[test]
    fn workflow_grants_are_scoped_to_plan_generation() {
        let secret = "synthetic-test-service-signing-secret";
        let token = workflow_trigger_token(secret, "tenant", "workflow", "actor", 3);
        let decoded = URL_SAFE_NO_PAD.decode(token).unwrap();
        let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
        assert!(
            hmac::verify(
                &key,
                &workflow_scope_message("tenant", "workflow", "actor", 3),
                &decoded,
            )
            .is_ok()
        );
        assert!(
            hmac::verify(
                &key,
                &workflow_scope_message("tenant", "workflow", "actor", 4),
                &decoded,
            )
            .is_err()
        );
    }
}
