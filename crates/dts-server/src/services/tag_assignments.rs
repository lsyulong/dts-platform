// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dts_backend::{
    ProductResourceRepo,
    repo::tag_assignment::{
        TagAssignmentCommand, TagAssignmentFilter, TagAssignmentMutation, TagAssignmentPage,
        TagAssignmentRepo,
    },
};
use serde::Deserialize;

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product::catalog},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagAssignmentRequest {
    pub target_kind: String,
    pub target_id: String,
    pub idempotency_key: String,
}

#[derive(Clone)]
pub struct TagAssignmentService {
    assignments: Arc<dyn TagAssignmentRepo>,
    resources: Arc<dyn ProductResourceRepo>,
    auth: AuthService,
}

impl TagAssignmentService {
    pub fn new(
        assignments: Arc<dyn TagAssignmentRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        auth: AuthService,
    ) -> Self {
        Self {
            assignments,
            resources,
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

    async fn tag(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<dts_backend::ProductResource, AppError> {
        self.resources
            .get(&actor.tenant_id, "tag", id)
            .await?
            .ok_or_else(|| AppError::NotFound("标签不存在或不属于当前租户".into()))
    }

    fn target_module(target_kind: &str) -> Result<&'static catalog::ProductModule, AppError> {
        let module = catalog::module(target_kind)?;
        if module.resource_kind.as_deref() != Some(target_kind) {
            return Err(AppError::BadRequest("标签只能绑定实际业务资源".into()));
        }
        if target_kind == "tag" {
            return Err(AppError::BadRequest("标签不能绑定另一个标签".into()));
        }
        Ok(module)
    }

    fn validate_scope(scope: &str, target_kind: &str) -> Result<(), AppError> {
        let module = Self::target_module(target_kind)?;
        if scope != "all" && scope != module.group_id {
            return Err(AppError::BadRequest(format!("该标签仅适用于 {scope} 模块")));
        }
        Ok(())
    }

    pub async fn list(
        &self,
        actor: &AuthIdentity,
        tag_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<TagAssignmentPage, AppError> {
        self.permission(actor, false).await?;
        self.tag(actor, tag_id).await?;
        Ok(self
            .assignments
            .list(&TagAssignmentFilter {
                tenant_id: actor.tenant_id.clone(),
                tag_id: tag_id.into(),
                limit,
                offset,
            })
            .await?)
    }

    pub async fn change(
        &self,
        actor: &AuthIdentity,
        tag_id: &str,
        expected_version: i64,
        action: &str,
        request: TagAssignmentRequest,
    ) -> Result<TagAssignmentMutation, AppError> {
        self.permission(actor, true).await?;
        let tag = self.tag(actor, tag_id).await?;
        if tag.version != expected_version {
            return Err(AppError::Conflict("标签定义已更新，请刷新后重试".into()));
        }
        let scope = tag
            .properties
            .get("scope")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppError::BadRequest("标签适用范围无效".into()))?;
        if action == "assign" {
            Self::validate_scope(scope, &request.target_kind)?;
        } else {
            Self::target_module(&request.target_kind)?;
        }
        let command = TagAssignmentCommand {
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            tag_id: tag_id.into(),
            expected_tag_version: expected_version,
            target_kind: request.target_kind,
            target_id: request.target_id,
            idempotency_key: request.idempotency_key,
        };
        let result = match action {
            "assign" => self.assignments.assign(&command).await?,
            "unassign" => self.assignments.unassign(&command).await?,
            _ => return Err(AppError::BadRequest("标签不支持此操作".into())),
        };
        self.permission(actor, false).await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::TagAssignmentService;

    #[test]
    fn scope_must_match_the_targets_real_catalog_group() {
        assert!(TagAssignmentService::validate_scope("assets", "asset_entry").is_ok());
        assert!(TagAssignmentService::validate_scope("assets", "development_workflow").is_err());
        assert!(TagAssignmentService::validate_scope("all", "development_workflow").is_ok());
        assert!(TagAssignmentService::validate_scope("support", "tag").is_err());
        assert!(TagAssignmentService::target_module("development_workflow").is_ok());
    }
}
