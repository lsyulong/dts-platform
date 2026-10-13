// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_jobs::validate_idempotency_key},
};
use dts_backend::{
    ProductResource, ProductResourceRepo, ResourceWrite, UpdateResourceParams,
    repo::{
        metadata_catalog::{MetadataCatalogRepo, MetadataColumn, binding_matches},
        product_job::*,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

mod compiler;
mod definition;
mod result;
#[cfg(test)]
mod tests;
mod types;
pub use compiler::prepare_claimed;
pub use result::map_node_result;
pub use types::*;

#[derive(Clone)]
pub struct BiReportService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    auth: AuthService,
    access: QueryAccess,
}

impl BiReportService {
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
        self.auth.authorize(actor, "bi:read").await?;
        let source = self
            .resources
            .get(&actor.tenant_id, "bi_report", id)
            .await?
            .ok_or_else(|| AppError::NotFound("报表不存在于当前租户".into()))?;
        if source.version != version {
            return Err(AppError::Conflict("报表定义版本已改变".into()));
        }
        Ok(source)
    }

    async fn freeze(
        &self,
        actor: &AuthIdentity,
        source: &ProductResource,
    ) -> Result<ReportSnapshot, AppError> {
        let id = source.properties["metadata_object_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or_else(|| bad("报表须绑定实际采集的元数据对象"))?;
        let metadata = self
            .resources
            .get(&actor.tenant_id, "metadata_object", id)
            .await?
            .ok_or_else(|| AppError::NotFound("实际分析对象不存在于当前租户".into()))?;
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, id, metadata.version)
            .await?
            .filter(|binding| {
                binding.engine == "swanlake"
                    && binding.tenant_id == actor.tenant_id
                    && binding_matches(binding, &metadata.properties)
            })
            .ok_or_else(|| bad("实际分析对象没有有效内部 SwanLake 来源证明，请完成真实采集"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(metadata.properties["columns"].clone())
                .map_err(|_| bad("实际字段元数据不符合完整采集合同"))?;
        let report = definition::parse(&source.properties, &columns, &source.name)?;
        let frozen = ReportSnapshot {
            format_version: 1,
            executor: "bi_aggregate_v1".into(),
            tenant_id: actor.tenant_id.clone(),
            source_kind: "bi_report".into(),
            source_id: source.id.clone(),
            source_version: source.version,
            properties: source.properties.clone(),
            metadata: FrozenMetadata {
                id: metadata.id,
                version: metadata.version,
                properties: metadata.properties,
                binding,
                columns,
            },
            report,
        };
        compiler::sql(&frozen)?;
        Ok(frozen)
    }

    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        let frozen = self.freeze(actor, &source).await?;
        Ok(
            json!({"valid":true,"metadata_id":frozen.metadata.id,"metadata_version":frozen.metadata.version,
            "dimensions":frozen.report.dimensions,"measures":frozen.report.measures,"row_limit":frozen.report.row_limit,
            "engine_bound":self.access.tenant_id.as_deref()==Some(actor.tenant_id.as_str()),"executed":false,"schedule_registered":false}),
        )
    }

    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        self.auth.authorize(actor, "bi:write").await?;
        self.auth.authorize(actor, "bi:execute").await?;
        if !["draft", "inactive", "publish_failed"].contains(&source.status.as_str()) {
            return Err(AppError::Conflict("当前报表状态不能发布".into()));
        }
        self.freeze(actor, &source).await?;
        let updated = self
            .resources
            .update(
                &actor.tenant_id,
                "bi_report",
                id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: "published".into(),
                        properties: source.properties,
                        references: source.references,
                    },
                },
            )
            .await?;
        Ok(
            json!({"resource":updated,"publication_scope":"validated_bi_definition","executed":false,"schedule_registered":false}),
        )
    }

    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: BiRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        let source = self.source(actor, id, request.expected_version).await?;
        self.auth.authorize(actor, "bi:execute").await?;
        if source.status != "published" {
            return Err(AppError::Conflict(
                "请先发布经过实际元数据校验的报表定义".into(),
            ));
        }
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(AppError::Forbidden("当前租户未绑定 BI 查询引擎".into()));
        }
        validate_idempotency_key(&request.idempotency_key)?;
        let frozen = self.freeze(actor, &source).await?;
        let node = compiler::node(&frozen);
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: "bi_report".into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "execute_bi".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: json!(frozen),
                parameters: json!({}),
                can_retry: true,
                nodes: vec![node],
            })
            .await?)
    }
}

pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
