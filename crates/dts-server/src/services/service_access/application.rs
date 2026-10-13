use super::{ServiceAccessService, bad, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product::{ResourceInput, ResourceUpdate},
};
use chrono::{DateTime, Duration, Utc};
use dts_backend::{ProductResource, ResourcePage, repo::service_access::*};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationProperties {
    owner_id: Option<String>,
    #[serde(default)]
    allowed_origins: Vec<String>,
    credential_expires_at: Option<DateTime<Utc>>,
}

impl ServiceAccessService {
    pub async fn save_application(
        &self,
        actor: &AuthIdentity,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .application_params(actor, uuid::Uuid::new_v4().to_string(), None, input)
            .await?;
        Ok(self.repo.save_application(&params).await?)
    }
    pub async fn update_application(
        &self,
        actor: &AuthIdentity,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let params = self
            .application_params(
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
        Ok(self.repo.save_application(&params).await?)
    }
    pub async fn get_application(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        self.authority(actor).await?;
        self.repo
            .get_application(&actor.tenant_id, &actor.sub, id)
            .await?
            .map(|app| app.resource)
            .ok_or_else(|| AppError::NotFound("服务应用不存在或无权访问".into()))
    }
    pub async fn list_applications(
        &self,
        actor: &AuthIdentity,
        q: Option<String>,
        status: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ResourcePage, AppError> {
        self.authority(actor).await?;
        validation::pagination(limit, offset)?;
        Ok(self
            .repo
            .list_applications(&ServiceResourceFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                approvals: false,
                status,
                q,
                limit,
                offset,
            })
            .await?)
    }
    pub async fn archive_application(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<ProductResource, AppError> {
        self.authority(actor).await?;
        Ok(self
            .repo
            .archive_application(&ArchiveServiceApplicationParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                application_id: id.into(),
                expected_version: version,
            })
            .await?)
    }
    async fn application_params(
        &self,
        actor: &AuthIdentity,
        id: String,
        expected: Option<i64>,
        input: ResourceInput,
    ) -> Result<SaveServiceApplicationParams, AppError> {
        let authority = self.authority(actor).await?;
        if input.parent_id.is_some() {
            return Err(bad("服务应用不支持父子层级"));
        }
        let properties: ApplicationProperties = serde_json::from_value(input.properties)
            .map_err(|_| bad("服务应用字段、来源或时间类型无效"))?;
        let owner = properties
            .owner_id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| actor.sub.clone());
        if owner != actor.sub && !authority.can_approve {
            return Err(denied("普通用户不能将应用负责人指定为其他用户"));
        }
        let detail = self
            .users
            .find_detail_by_id(&owner)
            .await?
            .filter(|user| user.tenant_id == actor.tenant_id)
            .ok_or_else(|| bad("服务应用负责人不是当前租户真实用户"))?;
        self.users
            .find_by_tenant_and_username(&actor.tenant_id, &detail.username)
            .await?
            .filter(|user| user.id == owner && user.status == "active")
            .ok_or_else(|| bad("服务应用负责人已停用或不存在"))?;
        let mut origins = Vec::new();
        for origin in properties.allowed_origins {
            origins.push(validation::canonical_origin(&origin)?);
        }
        let params = SaveServiceApplicationParams {
            id,
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            expected_version: expected,
            code: input.code,
            name: input.name,
            description: input.description,
            owner_id: owner,
            allowed_origins: origins,
            credential_expires_at: properties
                .credential_expires_at
                .unwrap_or_else(|| Utc::now() + Duration::days(30)),
        };
        validation::save_application(&params)?;
        Ok(params)
    }
}
