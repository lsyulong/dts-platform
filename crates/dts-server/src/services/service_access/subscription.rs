use super::{ServiceAccessService, bad, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product::{ResourceAction, ResourceInput, ResourceUpdate},
};
use chrono::{DateTime, Duration, Utc};
use dts_backend::{ProductResource, ResourcePage, repo::service_access::*};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionProperties {
    application_id: String,
    api_id: String,
    purpose: String,
    expires_at: Option<DateTime<Utc>>,
    owner_id: Option<String>,
    requester_id: Option<String>,
    api_version: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionPayload {
    idempotency_key: String,
    #[serde(default)]
    reason: String,
    expires_at: Option<DateTime<Utc>>,
}

impl ServiceAccessService {
    pub async fn save_subscription(
        &self,
        actor: &AuthIdentity,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .subscription_params(actor, uuid::Uuid::new_v4().to_string(), None, input)
            .await?;
        Ok(self.repo.save_subscription(&params).await?)
    }
    pub async fn update_subscription(
        &self,
        actor: &AuthIdentity,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .subscription_params(
                actor,
                id.into(),
                Some(input.expected_version),
                ResourceInput {
                    code: input.code,
                    name: input.name,
                    description: input.description,
                    parent_id: input.parent_id,
                    properties: input.properties,
                },
            )
            .await?;
        Ok(self.repo.save_subscription(&params).await?)
    }
    pub async fn get_subscription(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        self.authority(actor).await?;
        self.repo
            .get_subscription(&actor.tenant_id, &actor.sub, id)
            .await?
            .map(|sub| sub.resource)
            .ok_or_else(|| AppError::NotFound("服务订阅不存在或无权访问".into()))
    }
    pub async fn list_subscriptions(
        &self,
        actor: &AuthIdentity,
        approvals: bool,
        q: Option<String>,
        status: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourcePage, AppError> {
        let authority = self.authority(actor).await?;
        if approvals && !authority.can_approve {
            return Err(denied("服务审批列表要求当前明确services:approve权限"));
        }
        validation::pagination(limit, offset)?;
        Ok(self
            .repo
            .list_subscriptions(&ServiceResourceFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                approvals,
                status,
                q,
                limit,
                offset,
            })
            .await?)
    }
    pub async fn subscription_action(
        &self,
        actor: &AuthIdentity,
        id: &str,
        action: &str,
        input: ResourceAction,
    ) -> Result<ServiceSubscriptionDecisionResult, AppError> {
        let decision = match action {
            "submit" => ServiceSubscriptionDecision::Submit,
            "approve" => ServiceSubscriptionDecision::Approve,
            "reject" => ServiceSubscriptionDecision::Reject,
            "revoke" => ServiceSubscriptionDecision::Revoke,
            _ => return Err(bad("服务订阅动作不受支持")),
        };
        let authority = self.authority(actor).await?;
        if decision.needs_approver() && !authority.can_approve {
            return Err(denied("服务审批动作要求当前明确services:approve权限"));
        }
        let payload: DecisionPayload = serde_json::from_value(input.payload)
            .map_err(|_| bad("服务审批参数只接受idempotency_key/reason/可选expires_at"))?;
        let params = DecideServiceSubscriptionParams {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            subscription_id: id.into(),
            expected_version: input.expected_version,
            decision,
            reason: payload.reason,
            expires_at: payload.expires_at,
            idempotency_key: payload.idempotency_key,
        };
        validation::decision_semantics(&params)?;
        Ok(self.repo.decide_subscription(&params).await?)
    }
    async fn subscription_params(
        &self,
        actor: &AuthIdentity,
        id: String,
        expected: Option<i64>,
        input: ResourceInput,
    ) -> Result<SaveServiceSubscriptionParams, AppError> {
        self.authority(actor).await?;
        if input.parent_id.is_some() {
            return Err(bad("服务订阅不支持父子层级"));
        }
        let properties: SubscriptionProperties =
            serde_json::from_value(input.properties).map_err(|_| bad("服务订阅字段或类型无效"))?;
        let application = self
            .repo
            .get_application(&actor.tenant_id, &actor.sub, &properties.application_id)
            .await?
            .filter(|app| app.resource.status == "active")
            .ok_or_else(|| bad("服务订阅须绑定当前租户可管理且启用的真实应用"))?;
        let api = self
            .resources
            .get(&actor.tenant_id, "data_api", &properties.api_id)
            .await?
            .filter(|api| {
                api.tenant_id == actor.tenant_id
                    && api.kind == "data_api"
                    && api.status == "published"
            })
            .ok_or_else(|| bad("订阅须绑定当前租户真实已发布数据API"))?;
        // Read-only fields from an edit roundtrip are checked but never trusted.
        if properties
            .owner_id
            .as_deref()
            .is_some_and(|id| id != application.owner_id)
            || properties
                .api_version
                .is_some_and(|version| version != api.version)
        {
            return Err(AppError::Conflict(
                "服务订阅冻结的负责人或API版本已变化".into(),
            ));
        }
        if expected.is_none()
            && properties
                .requester_id
                .as_deref()
                .is_some_and(|id| id != actor.sub)
        {
            return Err(denied("服务申请人只能来自当前会话"));
        }
        let params = SaveServiceSubscriptionParams {
            id,
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            expected_version: expected,
            code: input.code,
            name: input.name,
            description: input.description,
            application_id: application.resource.id,
            api_id: api.id,
            api_version: api.version,
            purpose: properties.purpose,
            expires_at: properties
                .expires_at
                .unwrap_or_else(|| Utc::now() + Duration::days(30)),
        };
        validation::save_subscription(&params)?;
        Ok(params)
    }
}
