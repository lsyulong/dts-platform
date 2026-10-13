// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_jobs::validate_idempotency_key},
};
use dts_backend::{ProductResource, ProductResourceRepo, repo::product_job::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

mod compiler;
mod result;
#[cfg(test)]
mod tests;

pub use compiler::prepare_claimed;
pub use result::decode_result;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataRunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
}

#[derive(Clone)]
pub struct MetadataService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    auth: AuthService,
    access: QueryAccess,
}

impl MetadataService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        auth: AuthService,
        access: QueryAccess,
    ) -> Self {
        Self {
            resources,
            jobs,
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
        self.auth.authorize(actor, "metadata:read").await?;
        let source = self
            .resources
            .get(&actor.tenant_id, "metadata_collection", id)
            .await?
            .ok_or_else(|| AppError::NotFound("采集定义不存在于当前租户".into()))?;
        if source.version != version || !["draft", "published"].contains(&source.status.as_str()) {
            return Err(AppError::Conflict("采集定义版本已改变或已停用".into()));
        }
        Ok(source)
    }

    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, id, version).await?;
        let scope = compiler::scope(&source.properties)?;
        compiler::query(&scope)?;
        Ok(
            json!({"valid":true,"scope":scope,"engine_bound":self.access.tenant_id.as_deref()==Some(actor.tenant_id.as_str()),"executed":false,"schedule_registered":false}),
        )
    }

    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: MetadataRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        let source = self.source(actor, id, request.expected_version).await?;
        self.auth.authorize(actor, "metadata:execute").await?;
        let scope = compiler::scope(&source.properties)?;
        match self.access.tenant_id.as_deref() {
            Some(tenant) if tenant == actor.tenant_id => {},
            Some(_) => {
                return Err(AppError::Forbidden(
                    "当前租户未绑定 SwanLake 元数据来源".into(),
                ));
            },
            None => return Err(bad("SwanLake 未启用或没有租户绑定")),
        }
        validate_idempotency_key(&request.idempotency_key)?;
        compiler::query(&scope)?;
        let snapshot = json!({"format_version":1,"executor":"metadata_catalog_v1","tenant_id":actor.tenant_id,
            "source_kind":"metadata_collection","source_id":source.id,"source_version":source.version,"properties":source.properties,"scope":scope});
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: "metadata_collection".into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "collect_metadata".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: snapshot,
                parameters: json!({}),
                can_retry: true,
                nodes: vec![compiler::node(&scope)],
            })
            .await?)
    }
}

pub(super) fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
