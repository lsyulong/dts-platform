use super::{
    StandardGovernanceService, StandardRevisionsQuery, StandardValidation, WithdrawStandardRequest,
    validation,
};
use crate::models::{auth::AuthIdentity, error::AppError};
use dts_backend::repo::standard_governance::*;
use serde_json::Value;

impl StandardGovernanceService {
    async fn prepare_standard(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<(dts_backend::ProductResource, Value, Vec<FrozenStandardRef>), AppError> {
        self.authorize(actor, &["standards:read"]).await?;
        validation::standard_kind(kind)?;
        let resource = self.resource(actor, kind, id, version).await?;
        let normalized = validation::normalized_standard(kind, &resource.properties)?;
        let mut dependencies = Vec::new();
        if kind == "field_standard" {
            for (key, child_kind) in [("unit", "unit_standard"), ("enum", "enum_standard")] {
                if let Some(child_id) = normalized[format!("{key}_id")].as_str() {
                    let child_version = normalized[format!("{key}_version")]
                        .as_i64()
                        .ok_or_else(|| validation::bad("子标准版本缺失"))?;
                    dependencies.push(
                        self.active_revision(&actor.tenant_id, child_kind, child_id, child_version)
                            .await?
                            .reference(),
                    );
                }
            }
        }
        Ok((resource, normalized, dependencies))
    }
    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<StandardValidation, AppError> {
        self.authorize(actor, &["standards:execute"]).await?;
        let (_, _, dependencies) = self.prepare_standard(actor, kind, id, version).await?;
        self.authorize(actor, &["standards:read", "standards:execute"])
            .await?;
        Ok(StandardValidation {
            valid: true,
            validation_scope: "definition_and_published_child_versions",
            executed: false,
            dependency_versions: dependencies,
        })
    }
    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<StandardTransitionResult, AppError> {
        self.authorize(actor, &["standards:write", "standards:execute"])
            .await?;
        let (source, properties, children) =
            self.prepare_standard(actor, kind, id, version).await?;
        if !matches!(source.status.as_str(), "draft" | "publish_failed") {
            return Err(AppError::Conflict("只可发布已保存的标准草稿".into()));
        }
        Ok(self
            .repo
            .transition(&StandardTransitionParams {
                scope: scope(actor, kind, id, version),
                command: StandardCommand::Publish {
                    expected_properties: source.properties,
                    normalized_properties: properties,
                    children,
                },
            })
            .await?)
    }
    pub async fn revise(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<StandardTransitionResult, AppError> {
        self.authorize(actor, &["standards:write", "standards:execute"])
            .await?;
        validation::standard_kind(kind)?;
        let source = self.resource(actor, kind, id, version).await?;
        if source.status != "published" {
            return Err(AppError::Conflict(
                "只可为当前已发布定义创建下一版草稿".into(),
            ));
        }
        Ok(self
            .repo
            .transition(&StandardTransitionParams {
                scope: scope(actor, kind, id, version),
                command: StandardCommand::Revise,
            })
            .await?)
    }
    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        request: WithdrawStandardRequest,
    ) -> Result<StandardTransitionResult, AppError> {
        self.authorize(actor, &["standards:write", "standards:execute"])
            .await?;
        validation::standard_kind(kind)?;
        if request.reason.trim().is_empty()
            || request.reason.len() > 2000
            || request.reason.chars().any(char::is_control)
            || request.revision_version < 1
            || request.expected_state_version < 1
        {
            return Err(validation::bad("撤回需要真实版本和 1 至 2000 字符原因"));
        }
        Ok(self
            .repo
            .transition(&StandardTransitionParams {
                scope: scope(actor, kind, id, request.expected_version),
                command: StandardCommand::Withdraw {
                    revision_version: request.revision_version,
                    expected_state_version: request.expected_state_version,
                    reason: request.reason.trim().into(),
                },
            })
            .await?)
    }
    pub async fn revisions(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        query: StandardRevisionsQuery,
    ) -> Result<StandardRevisionPage, AppError> {
        self.authorize(actor, &["standards:read"]).await?;
        validation::standard_kind(kind)?;
        validation::identifier(id)?;
        let limit = query.limit.unwrap_or(20);
        if !(1..=100).contains(&limit) || query.offset.unwrap_or(0) > 1_000_000 {
            return Err(validation::bad("版本分页范围无效"));
        }
        let result = self
            .repo
            .revisions(&StandardRevisionFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                kind: kind.into(),
                id: id.into(),
                limit,
                offset: query.offset.unwrap_or(0),
            })
            .await?;
        self.authorize(actor, &["standards:read"]).await?;
        Ok(result)
    }
}
fn scope(actor: &AuthIdentity, kind: &str, id: &str, version: i64) -> StandardScope {
    StandardScope {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
        kind: kind.into(),
        id: id.into(),
        expected_version: version,
    }
}
