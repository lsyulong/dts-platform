use super::{
    bad, denied,
    input::{identifier, requested_columns, text},
};
use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::product::{ResourceAction, ResourceInput, ResourceUpdate},
};
use chrono::{DateTime, Duration, Utc};
use dts_backend::{
    ProductResource, ProductResourceRepo, ResourcePage,
    repo::{
        data_access::*,
        metadata_catalog::{MetadataCatalogRepo, MetadataColumn, binding_matches},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub struct DataAccessService {
    pub(super) repo: Arc<dyn DataAccessRepo>,
    pub(super) resources: Arc<dyn ProductResourceRepo>,
    pub(super) metadata: Arc<dyn MetadataCatalogRepo>,
    pub(super) access: QueryAccess,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionPayload {
    idempotency_key: String,
    #[serde(default)]
    decision_reason: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
}

impl DataAccessService {
    pub fn new(
        repo: Arc<dyn DataAccessRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        access: QueryAccess,
    ) -> Self {
        Self {
            repo,
            resources,
            metadata,
            access,
        }
    }

    pub async fn save_request(
        &self,
        actor: &AuthIdentity,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .params(actor, uuid::Uuid::new_v4().to_string(), None, input)
            .await?;
        Ok(self.repo.save_request(&params).await?)
    }

    pub async fn update_request(
        &self,
        actor: &AuthIdentity,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .params(
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
        Ok(self.repo.save_request(&params).await?)
    }

    pub async fn get_request(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        self.active(actor).await?;
        self.repo
            .get_request(&actor.tenant_id, id)
            .await?
            .filter(|request| {
                request.requester_id == actor.sub && request.resource.tenant_id == actor.tenant_id
            })
            .map(|request| request.resource)
            .ok_or_else(|| AppError::NotFound("当前用户的权限申请不存在".into()))
    }

    async fn params(
        &self,
        actor: &AuthIdentity,
        id: String,
        expected_version: Option<i64>,
        input: ResourceInput,
    ) -> Result<SaveDataAccessRequestParams, AppError> {
        self.active(actor).await?;
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(denied("当前租户未绑定可申请访问的分析引擎"));
        }
        if input.parent_id.is_some()
            || !identifier(&input.code)
            || input.name.trim().is_empty()
            || input.name.chars().count() > 128
            || input.name.chars().any(char::is_control)
        {
            return Err(bad("申请编码、名称无效或使用了不支持的父对象"));
        }
        let properties = input
            .properties
            .as_object()
            .ok_or_else(|| bad("申请属性必须是对象"))?;
        if properties.keys().any(|key| {
            ![
                "object_id",
                "columns",
                "purpose",
                "expires_at",
                "requester_id",
                "metadata_version",
            ]
            .contains(&key.as_str())
        }) {
            return Err(bad("数据权限申请包含未知字段"));
        }
        if properties
            .get("requester_id")
            .is_some_and(|value| value.as_str() != Some(actor.sub.as_str()))
        {
            return Err(denied("申请人必须是当前真实登录用户"));
        }
        let metadata_id = text(&input.properties, "object_id")?;
        let resource = self
            .resources
            .get(&actor.tenant_id, "metadata_object", metadata_id)
            .await?
            .filter(|row| {
                row.tenant_id == actor.tenant_id
                    && row.kind == "metadata_object"
                    && row.id == metadata_id
            })
            .ok_or_else(|| bad("元数据对象不存在于当前租户"))?;
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, &resource.id, resource.version)
            .await?
            .filter(|binding| {
                binding.engine == "swanlake"
                    && binding.tenant_id == actor.tenant_id
                    && binding.object_version == resource.version
                    && binding_matches(binding, &resource.properties)
            })
            .ok_or_else(|| bad("元数据对象没有当前有效的真实采集证明"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(resource.properties["columns"].clone())
                .map_err(|_| bad("实际采集字段格式无效"))?;
        let requested = requested_columns(input.properties.get("columns"), &columns)?;
        let expires = match input
            .properties
            .get("expires_at")
            .filter(|value| !value.is_null() && **value != Value::String(String::new()))
        {
            None => Utc::now() + Duration::days(30),
            Some(value) => DateTime::parse_from_rfc3339(
                value
                    .as_str()
                    .ok_or_else(|| bad("失效时间必须是带时区时间字符串"))?,
            )
            .map_err(|_| bad("失效时间须包含日期时间及UTC偏移"))?
            .with_timezone(&Utc),
        };
        let params = SaveDataAccessRequestParams {
            id,
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            expected_version,
            code: input.code,
            name: input.name.trim().into(),
            description: input.description,
            target: DataAccessTarget {
                metadata_id: resource.id,
                binding,
                columns,
            },
            requested_columns: requested,
            purpose: text(&input.properties, "purpose")?.into(),
            expires_at: expires,
        };
        validation::save(&params)?;
        Ok(params)
    }

    pub async fn action(
        &self,
        actor: &AuthIdentity,
        id: &str,
        action: &str,
        input: ResourceAction,
    ) -> Result<DataAccessDecisionResult, AppError> {
        let decision = match action {
            "submit" => DataAccessDecision::Submit,
            "approve" => DataAccessDecision::Approve,
            "reject" => DataAccessDecision::Reject,
            "withdraw" => DataAccessDecision::Withdraw,
            "revoke" => DataAccessDecision::Revoke,
            _ => return Err(bad("数据权限申请不支持此动作")),
        };
        let authority = self.active(actor).await?;
        if decision.needs_approver() && !authority.can_approve {
            return Err(denied("审批或撤销授权需要明确的data_security:approve权限"));
        }
        let payload: DecisionPayload = serde_json::from_value(input.payload)
            .map_err(|_| bad("权限动作参数需包含幂等键及受控审批意见/失效时间"))?;
        if payload
            .reason
            .as_ref()
            .zip(payload.decision_reason.as_ref())
            .is_some_and(|(a, b)| a != b)
        {
            return Err(bad("审批意见字段相互冲突"));
        }
        let params = DecideDataAccessParams {
            tenant_id: actor.tenant_id.clone(),
            request_id: id.into(),
            expected_version: input.expected_version,
            actor_id: actor.sub.clone(),
            decision,
            reason: payload
                .decision_reason
                .or(payload.reason)
                .unwrap_or_default(),
            expires_at: payload.expires_at,
            idempotency_key: payload.idempotency_key,
        };
        validation::decision_request(&params)?;
        // Repository reads a matching durable receipt before mutable state/version.
        Ok(self.repo.decide(&params).await?)
    }

    pub async fn list_approvals(
        &self,
        actor: &AuthIdentity,
        status: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourcePage, AppError> {
        if !self.active(actor).await?.can_approve {
            return Err(denied("审批列表需要明确审批权限"));
        }
        self.page(&status, limit)?;
        let mut page = self
            .repo
            .list_requests(&DataAccessRequestFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                approvals: true,
                status,
                limit,
                offset,
            })
            .await?;
        for resource in &mut page.data {
            resource.kind = "data_access_approval".into();
            let properties = resource
                .properties
                .as_object_mut()
                .ok_or_else(|| AppError::Internal("实际申请属性格式无效".into()))?;
            properties.insert("request_id".into(), json!(resource.id));
            properties.insert("requester_id".into(), json!(resource.created_by));
        }
        Ok(page)
    }

    pub async fn list_requests(
        &self,
        actor: &AuthIdentity,
        status: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourcePage, AppError> {
        self.active(actor).await?;
        self.page(&status, limit)?;
        Ok(self
            .repo
            .list_requests(&DataAccessRequestFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                approvals: false,
                status,
                limit,
                offset,
            })
            .await?)
    }

    pub async fn list_audit(
        &self,
        actor: &AuthIdentity,
        limit: u32,
        offset: u32,
    ) -> Result<DataAccessAuditPage, AppError> {
        self.active(actor).await?;
        self.page(&None, limit)?;
        Ok(self
            .repo
            .list_audit(&DataAccessAuditFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                limit,
                offset,
            })
            .await?)
    }

    pub(super) async fn active(
        &self,
        actor: &AuthIdentity,
    ) -> Result<DataAccessAuthority, AppError> {
        let authority = self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?;
        if authority.active {
            Ok(authority)
        } else {
            Err(denied("当前租户用户已停用或不存在"))
        }
    }
    fn page(&self, status: &Option<String>, limit: u32) -> Result<(), AppError> {
        if !(1..=100).contains(&limit)
            || status.as_ref().is_some_and(|status| {
                ![
                    "draft",
                    "pending",
                    "approved",
                    "rejected",
                    "withdrawn",
                    "revoked",
                ]
                .contains(&status.as_str())
            })
        {
            Err(bad("申请分页或状态筛选无效"))
        } else {
            Ok(())
        }
    }
}
