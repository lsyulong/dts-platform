use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product::{catalog, references, validation},
};
use dts_backend::{ProductResource, ResourceReference, repo::development_revisions::*};
use serde::Deserialize;
use std::sync::Arc;
mod diff;
pub use diff::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveDevelopmentRevisionRequest {
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreDevelopmentRevisionRequest {
    pub expected_version: i64,
    pub reason: String,
    pub idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentRevisionQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentRevisionDiffQuery {
    pub from_revision_id: String,
    pub to_revision_id: Option<String>,
}

#[derive(Clone)]
pub struct DevelopmentRevisionsService {
    repo: Arc<dyn DevelopmentRevisionRepo>,
}
impl DevelopmentRevisionsService {
    pub fn new(repo: Arc<dyn DevelopmentRevisionRepo>) -> Self {
        Self { repo }
    }
    pub fn handles(kind: &str) -> bool {
        matches!(kind, "sql_document" | "development_workflow")
    }
    pub async fn save(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        r: SaveDevelopmentRevisionRequest,
    ) -> Result<SaveDevelopmentRevisionResult, AppError> {
        let scope = scope(actor, kind, id);
        let current = self.repo.current(&scope).await?;
        let validation = if current.version == r.expected_version {
            Some(validated(&current)?)
        } else {
            None
        };
        Ok(self
            .repo
            .save(&SaveDevelopmentRevisionParams {
                scope,
                expected_version: r.expected_version,
                reason: r.reason,
                idempotency_key: r.idempotency_key,
                validation,
            })
            .await?)
    }
    pub async fn history(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        q: DevelopmentRevisionQuery,
    ) -> Result<DevelopmentRevisionPage, AppError> {
        Ok(self
            .repo
            .history(&DevelopmentRevisionFilter {
                scope: scope(actor, kind, id),
                limit: q.limit.unwrap_or(20),
                offset: q.offset.unwrap_or(0),
            })
            .await?)
    }
    pub async fn detail(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        revision_id: &str,
    ) -> Result<DevelopmentRevision, AppError> {
        Ok(self
            .repo
            .detail(&ReadDevelopmentRevisionParams {
                scope: scope(actor, kind, id),
                revision_id: revision_id.into(),
            })
            .await?)
    }
    pub async fn restore(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        revision_id: &str,
        r: RestoreDevelopmentRevisionRequest,
    ) -> Result<RestoreDevelopmentRevisionResult, AppError> {
        let scope = scope(actor, kind, id);
        let revision = self
            .repo
            .detail(&ReadDevelopmentRevisionParams {
                scope: scope.clone(),
                revision_id: revision_id.into(),
            })
            .await?;
        let validation = validated(&revision.snapshot)?;
        Ok(self
            .repo
            .restore(&RestoreDevelopmentRevisionParams {
                scope,
                revision_id: revision_id.into(),
                expected_version: r.expected_version,
                reason: r.reason,
                idempotency_key: r.idempotency_key,
                validation,
            })
            .await?)
    }
    pub async fn diff(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        q: DevelopmentRevisionDiffQuery,
    ) -> Result<DevelopmentRevisionDiff, AppError> {
        let from = self.detail(actor, kind, id, &q.from_revision_id).await?;
        let (to, to_revision_id) = match q.to_revision_id {
            Some(revision_id) => {
                let to = self.detail(actor, kind, id, &revision_id).await?;
                (to.snapshot, Some(revision_id))
            },
            None => (self.repo.current(&scope(actor, kind, id)).await?, None),
        };
        diff::compare(from, to, to_revision_id)
    }
}
fn scope(actor: &AuthIdentity, kind: &str, id: &str) -> DevelopmentRevisionScope {
    DevelopmentRevisionScope {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
        kind: kind.into(),
        resource_id: id.into(),
    }
}
fn validated(snapshot: &ProductResource) -> Result<ValidatedDevelopmentDefinition, AppError> {
    if !DevelopmentRevisionsService::handles(&snapshot.kind) {
        return Err(AppError::BadRequest("修订种类不支持".into()));
    }
    let module = catalog::module(&snapshot.kind)?;
    validation::properties(module, &snapshot.properties, snapshot.parent_id.as_deref())?;
    crate::services::product_jobs::validate_definition(&snapshot.kind, &snapshot.properties)?;
    let mut expected: Vec<ResourceReference> = references::extract(module, &snapshot.properties)?;
    expected.sort_by(|a, b| {
        (&a.field, &a.target_kind, &a.target_id).cmp(&(&b.field, &b.target_kind, &b.target_id))
    });
    let actual =
        dts_backend::repo::development_revisions::validation::canonical_snapshot(snapshot)?;
    if expected != actual.references {
        return Err(AppError::Conflict(
            "定义的实际引用与当前目录字段不一致".into(),
        ));
    }
    if !validation::identifier(&snapshot.code)
        || snapshot.name.trim().is_empty()
        || snapshot.name.chars().count() > 128
        || snapshot.name.chars().any(char::is_control)
        || snapshot
            .description
            .as_ref()
            .is_some_and(|v| v.len() > 8192)
    {
        return Err(AppError::BadRequest("修订定义名称、编码或说明无效".into()));
    }
    Ok(ValidatedDevelopmentDefinition::from_validated_snapshot(
        snapshot,
    )?)
}

#[cfg(test)]
mod tests;
