use std::sync::Arc;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use dts_backend::{
    BackendError, CreateResourceParams, ProductResource, ProductResourceRepo, ResourceReference,
    ResourceWrite, UpdateResourceParams,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::scheduler_trigger,
    config::SchedulerConfig,
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_jobs},
};

const SCHEDULE_KIND: &str = "development_schedule";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleActionRequest {
    pub idempotency_key: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentSchedulePlan {
    pub workflow_id: String,
    pub plan_version: u64,
    pub source_version: i64,
    pub cron_expression: String,
    pub timezone: String,
    pub retry_limit: u64,
    pub authorized_by: String,
    pub idempotency_key: String,
    pub last_operation: String,
    pub reason: String,
    pub scheduler_eid: Option<String>,
    pub last_error: Option<String>,
    pub changed_at: String,
}

#[derive(Debug, Serialize)]
pub struct DevelopmentScheduleStatus {
    pub scheduler_enabled: bool,
    pub state: String,
    pub plan: Option<DevelopmentSchedulePlan>,
}

#[derive(Debug, Serialize)]
pub struct DevelopmentScheduleResult {
    pub state: String,
    pub plan: DevelopmentSchedulePlan,
    pub replayed: bool,
    pub remote_confirmed: bool,
}

#[derive(Clone)]
pub struct DevelopmentScheduleService {
    resources: Arc<dyn ProductResourceRepo>,
    auth: AuthService,
    config: SchedulerConfig,
}

impl DevelopmentScheduleService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        auth: AuthService,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            resources,
            auth,
            config,
        }
    }

    pub async fn status(
        &self,
        actor: &AuthIdentity,
        workflow_id: &str,
    ) -> Result<DevelopmentScheduleStatus, AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "development:read").await?;
        self.workflow(actor, workflow_id).await?;
        let stored = self.stored(&actor.tenant_id, workflow_id).await?;
        Ok(DevelopmentScheduleStatus {
            scheduler_enabled: self.config.enabled,
            state: stored
                .as_ref()
                .map(|record| record.status.clone())
                .unwrap_or_else(|| "not_registered".into()),
            plan: stored.map(|record| decode_plan(&record)).transpose()?,
        })
    }

    pub async fn register(
        &self,
        actor: &AuthIdentity,
        workflow_id: &str,
        expected_version: i64,
        request: ScheduleActionRequest,
    ) -> Result<DevelopmentScheduleResult, AppError> {
        self.authorize_control(actor).await?;
        scheduler_trigger::validate_service_config(&self.config)?;
        self.config
            .validate_instance()
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        validate_request(&request)?;
        let workflow = self.workflow(actor, workflow_id).await?;
        if workflow.version != expected_version {
            return Err(AppError::Conflict(
                "工作流版本已改变，请刷新后重新注册".into(),
            ));
        }
        if workflow.status != "published" {
            return Err(AppError::Conflict("仅已发布工作流可以注册周期计划".into()));
        }
        let definition = product_jobs::schedule::validate(&workflow.properties)?
            .ok_or_else(|| AppError::BadRequest("工作流没有完整的周期定义".into()))?;
        if !definition.scheduler_timezone_supported {
            return Err(AppError::BadRequest(
                "JiaScheduler 社区版当前仅能可靠注册 UTC 周期，请将时区改为 UTC".into(),
            ));
        }
        if definition.retry_limit != 0 {
            return Err(AppError::BadRequest(
                "JiaScheduler 回调尚未提供稳定运行标识，注册前请将自动重试次数设为 0".into(),
            ));
        }

        if let Some(current) = self.stored(&actor.tenant_id, workflow_id).await? {
            let current_plan = decode_plan(&current)?;
            if current_plan.idempotency_key == request.idempotency_key
                && current_plan.last_operation == "register_schedule"
            {
                return Ok(result(current, current_plan, true));
            }
            if matches!(current.status.as_str(), "registering" | "stopping") {
                return Err(AppError::Conflict("周期计划正在变更，请稍后刷新".into()));
            }
        }

        let plan_version = self
            .stored(&actor.tenant_id, workflow_id)
            .await?
            .map(|record| decode_plan(&record).map(|plan| plan.plan_version + 1))
            .transpose()?
            .unwrap_or(1);
        let plan = DevelopmentSchedulePlan {
            workflow_id: workflow_id.into(),
            plan_version,
            source_version: workflow.version,
            cron_expression: definition.cron_expression,
            timezone: definition.timezone,
            retry_limit: definition.retry_limit,
            authorized_by: actor.sub.clone(),
            idempotency_key: request.idempotency_key,
            last_operation: "register_schedule".into(),
            reason: request.reason.clone(),
            scheduler_eid: None,
            last_error: None,
            changed_at: Utc::now().to_rfc3339(),
        };
        let pending = self
            .save(actor, &workflow, "registering", plan, request.reason)
            .await?;
        let pending_plan = decode_plan(&pending)?;
        let grant = scheduler_trigger::workflow_trigger_token(
            &self.config.service_token,
            &actor.tenant_id,
            workflow_id,
            &actor.sub,
            pending_plan.plan_version,
        );
        let url = self
            .config
            .workflow_trigger_url(
                workflow_id,
                &actor.tenant_id,
                &actor.sub,
                pending_plan.plan_version,
            )
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        let remote = self
            .upsert_remote(
                &remote_key(&actor.tenant_id, workflow_id),
                &workflow.name,
                &url,
                &grant,
                &pending_plan.cron_expression,
            )
            .await;
        let (state, eid, error) = match remote {
            Ok(eid) => ("active", Some(eid), None),
            Err(error) => (
                "registration_unknown",
                None,
                Some(limit_error(&error.to_string())),
            ),
        };
        let finalized = self.finalize(actor, pending, state, eid, error).await?;
        let final_plan = decode_plan(&finalized)?;
        Ok(result(finalized, final_plan, false))
    }

    pub async fn unschedule(
        &self,
        actor: &AuthIdentity,
        workflow_id: &str,
        expected_version: i64,
        request: ScheduleActionRequest,
    ) -> Result<DevelopmentScheduleResult, AppError> {
        self.authorize_control(actor).await?;
        if !self.config.enabled {
            return Err(AppError::BadRequest("jiascheduler 集成未启用".into()));
        }
        validate_request(&request)?;
        let workflow = self.workflow(actor, workflow_id).await?;
        if workflow.version != expected_version {
            return Err(AppError::Conflict(
                "工作流版本已改变，请刷新后重新停用".into(),
            ));
        }
        let current = self
            .stored(&actor.tenant_id, workflow_id)
            .await?
            .ok_or_else(|| AppError::Conflict("周期计划尚未注册".into()))?;
        let mut plan = decode_plan(&current)?;
        if plan.idempotency_key == request.idempotency_key && plan.last_operation == "unschedule" {
            return Ok(result(current, plan, true));
        }
        if matches!(current.status.as_str(), "registering" | "stopping") {
            return Err(AppError::Conflict("周期计划正在变更，请稍后刷新".into()));
        }
        if current.status == "stopped" {
            return Err(AppError::Conflict("周期计划已经停用".into()));
        }
        plan.idempotency_key = request.idempotency_key;
        plan.last_operation = "unschedule".into();
        plan.reason = request.reason.clone();
        plan.authorized_by = actor.sub.clone();
        plan.last_error = None;
        plan.changed_at = Utc::now().to_rfc3339();
        let pending = self
            .update(actor, &workflow, current, "stopping", plan, request.reason)
            .await?;
        let remote = self
            .remove_remote(&remote_key(&actor.tenant_id, workflow_id))
            .await;
        let (state, error) = match remote {
            Ok(()) => ("stopped", None),
            Err(error) => ("stop_unknown", Some(limit_error(&error.to_string()))),
        };
        let finalized = self.finalize(actor, pending, state, None, error).await?;
        let final_plan = decode_plan(&finalized)?;
        Ok(result(finalized, final_plan, false))
    }

    pub(crate) async fn trigger_context(
        &self,
        tenant_id: &str,
        workflow_id: &str,
        actor_id: &str,
        plan_version: u64,
    ) -> Result<DevelopmentSchedulePlan, AppError> {
        let stored = self
            .stored(tenant_id, workflow_id)
            .await?
            .filter(|record| record.status == "active")
            .ok_or_else(|| AppError::Conflict("周期计划未处于启用状态".into()))?;
        let plan = decode_plan(&stored)?;
        if plan.authorized_by != actor_id || plan.plan_version != plan_version {
            return Err(AppError::Unauthorized("周期计划授权已失效".into()));
        }
        Ok(plan)
    }

    pub async fn ensure_can_withdraw(
        &self,
        actor: &AuthIdentity,
        workflow_id: &str,
    ) -> Result<(), AppError> {
        if let Some(schedule) = self.stored(&actor.tenant_id, workflow_id).await? {
            if schedule.status != "stopped" {
                return Err(AppError::Conflict(
                    "请先停用并确认周期计划，再撤回工作流".into(),
                ));
            }
        }
        Ok(())
    }

    async fn authorize_control(&self, actor: &AuthIdentity) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "development:execute").await
    }

    async fn workflow(
        &self,
        actor: &AuthIdentity,
        workflow_id: &str,
    ) -> Result<ProductResource, AppError> {
        self.resources
            .get(&actor.tenant_id, "development_workflow", workflow_id)
            .await?
            .ok_or_else(|| AppError::NotFound("开发工作流不存在".into()))
    }

    async fn stored(
        &self,
        tenant_id: &str,
        workflow_id: &str,
    ) -> Result<Option<ProductResource>, AppError> {
        Ok(self
            .resources
            .get(tenant_id, SCHEDULE_KIND, workflow_id)
            .await?)
    }

    async fn save(
        &self,
        actor: &AuthIdentity,
        workflow: &ProductResource,
        state: &str,
        plan: DevelopmentSchedulePlan,
        reason: String,
    ) -> Result<ProductResource, AppError> {
        let existing = self.stored(&actor.tenant_id, &workflow.id).await?;
        if let Some(existing) = existing {
            return self
                .update(actor, workflow, existing, state, plan, reason)
                .await;
        }
        let fields = schedule_fields(workflow, state, plan, reason)?;
        match self
            .resources
            .create(&CreateResourceParams {
                id: workflow.id.clone(),
                tenant_id: actor.tenant_id.clone(),
                kind: SCHEDULE_KIND.into(),
                actor_id: actor.sub.clone(),
                fields,
            })
            .await
        {
            Ok(record) => Ok(record),
            Err(BackendError::AlreadyExists(_) | BackendError::ConstraintViolation(_)) => Err(
                AppError::Conflict("周期计划被并发创建，请刷新后重试".into()),
            ),
            Err(error) => Err(error.into()),
        }
    }

    async fn update(
        &self,
        actor: &AuthIdentity,
        workflow: &ProductResource,
        current: ProductResource,
        state: &str,
        plan: DevelopmentSchedulePlan,
        reason: String,
    ) -> Result<ProductResource, AppError> {
        Ok(self
            .resources
            .update(
                &actor.tenant_id,
                SCHEDULE_KIND,
                &workflow.id,
                &UpdateResourceParams {
                    expected_version: current.version,
                    actor_id: actor.sub.clone(),
                    fields: schedule_fields(workflow, state, plan, reason)?,
                },
            )
            .await?)
    }

    async fn finalize(
        &self,
        actor: &AuthIdentity,
        current: ProductResource,
        state: &str,
        eid: Option<String>,
        error: Option<String>,
    ) -> Result<ProductResource, AppError> {
        let mut plan = decode_plan(&current)?;
        plan.scheduler_eid = eid.or(plan.scheduler_eid);
        plan.last_error = error;
        plan.changed_at = Utc::now().to_rfc3339();
        let workflow = self.workflow(actor, &plan.workflow_id).await?;
        self.update(actor, &workflow, current, state, plan, String::new())
            .await
    }

    async fn upsert_remote(
        &self,
        workflow_id: &str,
        name: &str,
        url: &str,
        token: &str,
        cron: &str,
    ) -> anyhow::Result<String> {
        let client = dts_scheduler_client::JiaSchedulerClient::login(
            &self.config.base_url,
            &self.config.username,
            &self.config.password,
        )
        .await?;
        let sync = dts_scheduler_client::TaskSync::new(client, &self.config.instance_id).await?;
        sync.upsert_job(workflow_id, name, url, token, Some(cron))
            .await
    }

    async fn remove_remote(&self, workflow_id: &str) -> anyhow::Result<()> {
        let client = dts_scheduler_client::JiaSchedulerClient::login(
            &self.config.base_url,
            &self.config.username,
            &self.config.password,
        )
        .await?;
        dts_scheduler_client::TaskSync::remove_task_timer(&client, workflow_id).await
    }
}

fn schedule_fields(
    workflow: &ProductResource,
    state: &str,
    plan: DevelopmentSchedulePlan,
    reason: String,
) -> Result<ResourceWrite, AppError> {
    let mut plan = plan;
    if !reason.is_empty() {
        plan.reason = reason;
    }
    let properties =
        serde_json::to_value(plan).map_err(|_| AppError::Internal("周期计划编码失败".into()))?;
    Ok(ResourceWrite {
        code: format!("schedule_{}", workflow.code),
        name: format!("{} · 周期计划", workflow.name),
        description: Some("开发工作流的持久化周期调度控制记录".into()),
        parent_id: None,
        status: state.into(),
        properties,
        references: vec![ResourceReference {
            field: "workflow_id".into(),
            target_kind: "development_workflow".into(),
            target_id: workflow.id.clone(),
        }],
    })
}

fn decode_plan(resource: &ProductResource) -> Result<DevelopmentSchedulePlan, AppError> {
    serde_json::from_value(resource.properties.clone())
        .map_err(|_| AppError::Internal("已保存的周期计划结构无效".into()))
}

fn result(
    resource: ProductResource,
    plan: DevelopmentSchedulePlan,
    replayed: bool,
) -> DevelopmentScheduleResult {
    DevelopmentScheduleResult {
        remote_confirmed: matches!(resource.status.as_str(), "active" | "stopped"),
        state: resource.status,
        plan,
        replayed,
    }
}

fn validate_request(request: &ScheduleActionRequest) -> Result<(), AppError> {
    product_jobs::validate_idempotency_key(&request.idempotency_key)?;
    let reason = request.reason.trim();
    if reason.is_empty() || reason.len() > 500 || reason.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "调度变更原因须为 1 至 500 个可见字符".into(),
        ));
    }
    Ok(())
}

fn limit_error(error: &str) -> String {
    error.chars().take(500).collect()
}

fn remote_key(tenant_id: &str, workflow_id: &str) -> String {
    let scoped = format!("{tenant_id}\0{workflow_id}");
    let digest = ring::digest::digest(&ring::digest::SHA256, scoped.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(digest.as_ref());
    format!("wf-{}", &encoded[..24])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_requests_require_stable_keys_and_a_real_reason() {
        assert!(
            validate_request(&ScheduleActionRequest {
                idempotency_key: "register:one".into(),
                reason: "上线每日结算".into(),
            })
            .is_ok()
        );
        assert!(
            validate_request(&ScheduleActionRequest {
                idempotency_key: "bad key".into(),
                reason: "上线".into(),
            })
            .is_err()
        );
        assert!(
            validate_request(&ScheduleActionRequest {
                idempotency_key: "register:one".into(),
                reason: "  ".into(),
            })
            .is_err()
        );
    }

    #[test]
    fn scheduler_identity_is_tenant_scoped_and_fits_jia_limits() {
        let first = remote_key("tenant-a", "workflow-id");
        let second = remote_key("tenant-b", "workflow-id");
        assert_ne!(first, second);
        assert!(first.len() <= 36);
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        );
    }
}
