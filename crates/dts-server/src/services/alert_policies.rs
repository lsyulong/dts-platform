// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dts_backend::{
    ProductResource, ProductResourceRepo, UserRepo,
    repo::alert_policy::{
        AlertPolicyEvaluation, AlertPolicyEvaluationFilter, AlertPolicyEvaluationPage,
        AlertPolicyRepo, EvaluateAlertPolicyParams,
    },
};
use ring::digest::{SHA256, digest};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_domain::access},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertPolicyActionRequest {
    pub idempotency_key: String,
}

#[derive(Clone)]
pub struct AlertPolicyService {
    policies: Arc<dyn AlertPolicyRepo>,
    resources: Arc<dyn ProductResourceRepo>,
    users: Arc<dyn UserRepo>,
    auth: AuthService,
}

struct PreparedAlertMessage {
    rendered: access::RenderedMessage,
    channel: String,
}

impl AlertPolicyService {
    pub fn new(
        policies: Arc<dyn AlertPolicyRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        users: Arc<dyn UserRepo>,
        auth: AuthService,
    ) -> Self {
        Self {
            policies,
            resources,
            users,
            auth,
        }
    }

    async fn permission(&self, actor: &AuthIdentity, write: bool) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "support:read").await?;
        if write {
            self.auth.authorize(actor, "support:write").await?;
        }
        Ok(())
    }

    async fn policy(&self, actor: &AuthIdentity, id: &str) -> Result<ProductResource, AppError> {
        let policy = self
            .resources
            .get(&actor.tenant_id, "alert_policy", id)
            .await?
            .ok_or_else(|| AppError::NotFound("告警策略不存在或不属于当前租户".into()))?;
        access::validate("alert_policy", &policy.properties)?;
        Ok(policy)
    }

    fn integer(properties: &Value, key: &str) -> Result<i64, AppError> {
        properties
            .get(key)
            .and_then(Value::as_i64)
            .ok_or_else(|| AppError::BadRequest(format!("告警策略 {key} 无效")))
    }

    async fn recipients(&self, tenant: &str, properties: &Value) -> Result<Vec<String>, AppError> {
        let values = properties
            .get("recipient_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::BadRequest("告警收件人无效".into()))?;
        let recipients = values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| AppError::BadRequest("告警收件人无效".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let active: HashSet<_> = self
            .users
            .list_all(Some(tenant), None, Some("active"))
            .await?
            .into_iter()
            .filter(|user| user.tenant_id == tenant && user.status == "active")
            .map(|user| user.id)
            .collect();
        if recipients.iter().any(|id| !active.contains(id)) {
            return Err(AppError::BadRequest(
                "告警收件人不存在、停用或不属于当前租户".into(),
            ));
        }
        Ok(recipients)
    }

    async fn message(
        &self,
        actor: &AuthIdentity,
        policy: &ProductResource,
        test: bool,
    ) -> Result<PreparedAlertMessage, AppError> {
        let event_type = policy.properties["event_type"].as_str().unwrap_or_default();
        let threshold = Self::integer(&policy.properties, "threshold")?;
        let window = Self::integer(&policy.properties, "window_seconds")?;
        let Some(template_id) = policy
            .properties
            .get("message_template_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Ok(PreparedAlertMessage {
                channel: "in_app".into(),
                rendered: access::RenderedMessage {
                    subject: if test {
                        format!("[测试] {}", policy.name)
                    } else {
                        format!("告警触发：{}", policy.name)
                    },
                    body: format!(
                        "策略 {} 监控事件 {}，在 {} 秒窗口内达到阈值 {}。请进入告警设置查看真实评估记录。",
                        policy.name, event_type, window, threshold
                    ),
                },
            });
        };
        let template = self
            .resources
            .get(&actor.tenant_id, "message_template", template_id)
            .await?
            .ok_or_else(|| AppError::BadRequest("消息模板不存在或不属于当前租户".into()))?;
        let allowed = json!({"policy_name":policy.name,"event_type":event_type,"threshold":threshold,"window_seconds":window,"test_delivery":test});
        let mut parameters = Map::new();
        for name in template
            .properties
            .get("variables")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = name
                .as_str()
                .ok_or_else(|| AppError::BadRequest("消息模板变量无效".into()))?;
            let value = allowed
                .get(name)
                .cloned()
                .ok_or_else(|| AppError::BadRequest(format!("告警模板变量 {name} 不受支持")))?;
            parameters.insert(name.into(), value);
        }
        Ok(PreparedAlertMessage {
            channel: template.properties["channel"]
                .as_str()
                .unwrap_or_default()
                .into(),
            rendered: access::render_template(&template.properties, &Value::Object(parameters))?,
        })
    }

    pub async fn execute(
        &self,
        actor: &AuthIdentity,
        id: &str,
        expected_version: i64,
        action: &str,
        request: AlertPolicyActionRequest,
    ) -> Result<AlertPolicyEvaluation, AppError> {
        self.permission(actor, true).await?;
        crate::services::product_jobs::validate_idempotency_key(&request.idempotency_key)?;
        let policy = self.policy(actor, id).await?;
        if policy.version != expected_version {
            return Err(AppError::Conflict(
                "告警策略版本已经变化，请刷新后重试".into(),
            ));
        }
        let test = match action {
            "evaluate" => false,
            "test_delivery" => true,
            _ => return Err(AppError::BadRequest("告警策略不支持此操作".into())),
        };
        if !test
            && !policy
                .properties
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return Err(AppError::Conflict("停用的告警策略不能执行评估".into()));
        }
        let recipients = self
            .recipients(&actor.tenant_id, &policy.properties)
            .await?;
        let message = self.message(actor, &policy, test).await?;
        let result = self
            .policies
            .evaluate(&EvaluateAlertPolicyParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                policy_id: id.into(),
                policy_version: expected_version,
                policy_name: policy.name,
                event_type: policy.properties["event_type"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                window_seconds: Self::integer(&policy.properties, "window_seconds")?,
                threshold: Self::integer(&policy.properties, "threshold")?,
                cooldown_seconds: Self::integer(&policy.properties, "cooldown_seconds")?,
                recipient_ids: recipients,
                subject: message.rendered.subject,
                body: message.rendered.body,
                channel: message.channel,
                idempotency_key: request.idempotency_key,
                test_delivery: test,
            })
            .await?;
        self.permission(actor, false).await?;
        Ok(result)
    }

    pub async fn execute_scheduled(
        &self,
        claim: &dts_backend::repo::alert_policy::ScheduledAlertPolicy,
    ) -> Result<i64, AppError> {
        let user = self
            .users
            .find_detail_by_id(&claim.actor_id)
            .await?
            .filter(|user| user.tenant_id == claim.tenant_id)
            .ok_or_else(|| {
                AppError::Unauthorized("告警策略创建人不存在、停用或已经离开租户".into())
            })?;
        let actor = AuthIdentity {
            sub: user.id,
            tenant_id: user.tenant_id,
            username: user.username,
        };
        let raw = format!(
            "{}\0{}\0{}\0{}",
            claim.tenant_id,
            claim.policy_id,
            claim.policy_version,
            claim.scheduled_for.to_rfc3339()
        );
        let key = format!(
            "auto-{}",
            URL_SAFE_NO_PAD.encode(digest(&SHA256, raw.as_bytes()).as_ref())
        );
        self.execute(
            &actor,
            &claim.policy_id,
            claim.policy_version,
            "evaluate",
            AlertPolicyActionRequest {
                idempotency_key: key,
            },
        )
        .await?;
        let policy = self.policy(&actor, &claim.policy_id).await?;
        let window = Self::integer(&policy.properties, "window_seconds")?;
        Ok(policy
            .properties
            .get("evaluation_interval_seconds")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| (window / 4).clamp(60, 3600)))
    }

    pub async fn list(
        &self,
        actor: &AuthIdentity,
        id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<AlertPolicyEvaluationPage, AppError> {
        self.permission(actor, false).await?;
        self.policy(actor, id).await?;
        Ok(self
            .policies
            .list(&AlertPolicyEvaluationFilter {
                tenant_id: actor.tenant_id.clone(),
                policy_id: id.into(),
                limit,
                offset,
            })
            .await?)
    }
}
