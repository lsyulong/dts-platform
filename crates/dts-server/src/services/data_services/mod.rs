// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_jobs::validate_idempotency_key},
};
use dts_backend::{
    ProductResource, ProductResourceRepo, ResourceReference, ResourceWrite, UpdateResourceParams,
    repo::{
        metadata_catalog::{MetadataCatalogRepo, MetadataColumn, binding_matches},
        product_job::*,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

mod compiler;
mod result;
mod result_policy;
mod schema;
mod sql;
mod types;
pub use compiler::prepare_claimed;
pub use result::map_node_result;
pub(crate) use result_policy::{
    map_protected_node_result, map_stored_node_result, protect_node_result,
    validate_native_node_result,
};
pub use types::*;

#[derive(Clone)]
pub struct DataApiService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    auth: AuthService,
    access: QueryAccess,
}

impl DataApiService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        auth: AuthService,
        access: QueryAccess,
    ) -> Self {
        Self {
            resources,
            jobs,
            metadata,
            auth,
            access,
        }
    }

    async fn source(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<ProductResource, AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "services:read").await?;
        let source = self
            .resources
            .get(&actor.tenant_id, "data_api", id)
            .await?
            .ok_or_else(|| AppError::NotFound("数据 API 不属于当前租户或不存在".into()))?;
        if source.version != version {
            return Err(AppError::Conflict("数据 API 定义版本已经改变".into()));
        }
        Ok(source)
    }

    async fn current_metadata(
        &self,
        actor: &AuthIdentity,
        properties: &Value,
    ) -> Result<ApiMetadata, AppError> {
        let id = properties["metadata_object_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or_else(|| bad("数据 API 须绑定实际采集元数据对象"))?;
        let resource = self
            .resources
            .get(&actor.tenant_id, "metadata_object", id)
            .await?
            .ok_or_else(|| AppError::NotFound("数据 API 实际来源不存在于当前租户".into()))?;
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, id, resource.version)
            .await?
            .filter(|proof| {
                proof.engine == "swanlake"
                    && proof.tenant_id == actor.tenant_id
                    && binding_matches(proof, &resource.properties)
            })
            .ok_or_else(|| bad("数据 API 实际来源没有当前有效内部采集证明"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(resource.properties["columns"].clone())
                .map_err(|_| bad("实际采集字段结构无效"))?;
        Ok(ApiMetadata {
            id: resource.id,
            version: resource.version,
            properties: resource.properties,
            binding,
            columns,
        })
    }

    async fn verify_publication(
        &self,
        actor: &AuthIdentity,
        source: &ProductResource,
    ) -> Result<ApiPublication, AppError> {
        let publication = compiler::publication(
            &source.properties,
            &actor.tenant_id,
            &source.id,
            source.version,
        )?;
        let proof = self
            .metadata
            .verified_binding(
                &actor.tenant_id,
                &publication.metadata.id,
                publication.metadata.version,
            )
            .await?;
        if proof.as_ref() != Some(&publication.metadata.binding)
            || !binding_matches(
                &publication.metadata.binding,
                &publication.metadata.properties,
            )
        {
            return Err(AppError::Conflict(
                "API 发布时的实际元数据证明已变化，请撤回后重新校验发布".into(),
            ));
        }
        Ok(publication)
    }

    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        let (metadata, api) = if source.status == "published" {
            let publication = self.verify_publication(actor, &source).await?;
            (publication.metadata, publication.api)
        } else {
            let metadata = self.current_metadata(actor, &source.properties).await?;
            let api = compiler::definition(&source.properties, &metadata)?;
            (metadata, api)
        };
        Ok(
            json!({"valid":true,"metadata_id":metadata.id,"metadata_version":metadata.version,"response_schema":api.response_schema,
            "row_limit":api.row_limit,"rate_limit":api.rate_limit,"engine_bound":self.access.tenant_id.as_deref()==Some(actor.tenant_id.as_str()),
            "invocation_scope":"authenticated_console_jobs","external_credentials_ready":false,"rate_limit_enforced":false,"executed":false}),
        )
    }

    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        self.auth.authorize(actor, "services:write").await?;
        self.auth.authorize(actor, "services:execute").await?;
        if !matches!(
            source.status.as_str(),
            "draft" | "inactive" | "publish_failed"
        ) {
            return Err(AppError::Conflict("仅草稿或停用的数据 API 可发布".into()));
        }
        let metadata = self.current_metadata(actor, &source.properties).await?;
        let api = compiler::definition(&source.properties, &metadata)?;
        let source_version = source
            .version
            .checked_add(1)
            .ok_or_else(|| AppError::Conflict("API 版本超过容量".into()))?;
        let publication = ApiPublication {
            format_version: 1,
            executor: "data_api_detail_v1".into(),
            tenant_id: actor.tenant_id.clone(),
            source_id: source.id.clone(),
            source_version,
            published_by: actor.sub.clone(),
            published_at: chrono::Utc::now().to_rfc3339(),
            metadata: metadata.clone(),
            api: api.clone(),
        };
        let mut properties = source.properties.clone();
        properties["parameters"] = api.parameters;
        properties["response_schema"] = json!(api.response_schema);
        properties["publication"] = json!(publication);
        compiler::publication(&properties, &actor.tenant_id, id, source_version)?;
        let mut references = source.references.clone();
        references.retain(|reference| reference.field != "metadata_object_id");
        references.push(ResourceReference {
            field: "metadata_object_id".into(),
            target_kind: "metadata_object".into(),
            target_id: metadata.id,
        });
        let published = self
            .write(actor, source, version, "published", properties, references)
            .await?;
        Ok(
            json!({"resource":published,"publication_scope":"validated_data_api_definition","invocation_scope":"authenticated_console_jobs",
            "external_credentials_ready":false,"rate_limit_enforced":false,"executed":false,"schedule_registered":false}),
        )
    }

    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        self.auth.authorize(actor, "services:write").await?;
        if source.status != "published" {
            return Err(AppError::Conflict("仅已发布 API 可撤回".into()));
        }
        let mut properties = source.properties.clone();
        if let Some(properties) = properties.as_object_mut() {
            properties.remove("publication");
        }
        let references = source.references.clone();
        let withdrawn = self
            .write(actor, source, version, "inactive", properties, references)
            .await?;
        Ok(json!({"resource":withdrawn,"publication_withdrawn":true}))
    }

    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: DataApiRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        let source = self.source(actor, id, request.expected_version).await?;
        self.auth.authorize(actor, "services:execute").await?;
        if source.status != "published" {
            return Err(AppError::Conflict(
                "请先发布经过实际来源和响应 schema 校验的数据 API".into(),
            ));
        }
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(AppError::Forbidden(
                "当前租户没有绑定 API 只读查询引擎".into(),
            ));
        }
        validate_idempotency_key(&request.idempotency_key)?;
        let publication = self.verify_publication(actor, &source).await?;
        let (parameters, _) = compiler::bind(&publication.api, &request.parameters)?;
        let node = compiler::node(&publication);
        let snapshot = ApiJobSnapshot {
            format_version: 1,
            executor: "data_api_detail_v1".into(),
            tenant_id: actor.tenant_id.clone(),
            source_kind: "data_api".into(),
            source_id: source.id.clone(),
            source_version: source.version,
            properties: source.properties,
            metadata: publication.metadata.clone(),
            publication,
        };
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: "data_api".into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "execute_data_api".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: json!(snapshot),
                parameters,
                can_retry: true,
                nodes: vec![node],
            })
            .await?)
    }

    pub async fn prepare_invocation(
        &self,
        actor: &AuthIdentity,
        id: &str,
        expected_version: i64,
        parameters: &Value,
    ) -> Result<PreparedDataApiCall, AppError> {
        let source = self.source(actor, id, expected_version).await?;
        self.auth.authorize(actor, "services:execute").await?;
        if source.status != "published" {
            return Err(AppError::Conflict("数据 API 已撤回或尚未发布".into()));
        }
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(AppError::Forbidden(
                "当前租户没有绑定数据 API 查询引擎".into(),
            ));
        }
        let publication = self.verify_publication(actor, &source).await?;
        let (_, sql) = compiler::bind(&publication.api, parameters)?;
        Ok(PreparedDataApiCall {
            actor_id: actor.sub.clone(),
            tenant_id: actor.tenant_id.clone(),
            source,
            publication,
            sql,
        })
    }

    pub async fn reauthorize_invocation(
        &self,
        actor: &AuthIdentity,
        prepared: &PreparedDataApiCall,
    ) -> Result<(), AppError> {
        if prepared.actor_id != actor.sub || prepared.tenant_id != actor.tenant_id {
            return Err(AppError::Forbidden(
                "数据 API 调用授权不属于当前执行身份".into(),
            ));
        }
        let current = self
            .source(actor, &prepared.source.id, prepared.source.version)
            .await?;
        self.auth.authorize(actor, "services:execute").await?;
        if current.status != "published"
            || current.properties != prepared.source.properties
            || self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str())
        {
            return Err(AppError::Conflict(
                "数据 API 已撤回、定义变化或引擎解绑".into(),
            ));
        }
        if self.verify_publication(actor, &current).await? != prepared.publication {
            return Err(AppError::Conflict("数据 API 冻结来源证明已变化".into()));
        }
        Ok(())
    }

    pub fn map_invocation(
        &self,
        prepared: &PreparedDataApiCall,
        result: ProtectedDataApiResult,
    ) -> Result<Value, AppError> {
        if result.invocation_identity.as_ref()
            != Some(&(
                prepared.tenant_id.clone(),
                prepared.actor_id.clone(),
                prepared.sql.clone(),
            ))
        {
            return Err(AppError::Forbidden(
                "API受保护输出不属于当前真实调用身份或SQL".into(),
            ));
        }
        result_policy::map_protected_result(&prepared.publication, result)
    }

    pub fn protect_invocation(
        &self,
        prepared: &PreparedDataApiCall,
        raw: crate::services::query_svc::QueryResult,
        masking: &crate::services::data_masking::MaskingService,
        plan: &crate::services::data_masking::MaskingPlan,
    ) -> Result<ProtectedDataApiResult, AppError> {
        if !plan.is_bound_to(&prepared.tenant_id, &prepared.actor_id, &prepared.sql) {
            return Err(AppError::Forbidden(
                "API脱敏计划不属于当前真实调用身份或SQL".into(),
            ));
        }
        let mut protected = result_policy::protect(&prepared.publication, raw, masking, plan)?;
        protected.invocation_identity = Some((
            prepared.tenant_id.clone(),
            prepared.actor_id.clone(),
            prepared.sql.clone(),
        ));
        Ok(protected)
    }

    async fn write(
        &self,
        actor: &AuthIdentity,
        source: ProductResource,
        version: i64,
        status: &str,
        properties: Value,
        references: Vec<ResourceReference>,
    ) -> Result<ProductResource, AppError> {
        Ok(self
            .resources
            .update(
                &actor.tenant_id,
                "data_api",
                &source.id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: status.into(),
                        properties,
                        references,
                    },
                },
            )
            .await?)
    }
}

pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}

#[cfg(test)]
mod tests;
