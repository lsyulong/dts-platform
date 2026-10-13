use super::{bad, definition, types::*};
use crate::{models::error::AppError, services::standards};
use dts_backend::{ProductResource, repo::standard_governance::StandardGovernanceRepo};

pub(super) fn validate_unit_pair(id: Option<&str>, version: Option<i64>) -> Result<(), AppError> {
    match (id, version) {
        (None, None) => Ok(()),
        (Some(id), Some(version)) if version > 0 => definition::text(id, 128),
        _ => Err(bad(
            "指标单位必须同时指定真实 unit_id 和已发布 unit_version",
        )),
    }
}
pub(super) async fn unit(
    repo: Option<&dyn StandardGovernanceRepo>,
    tenant: &str,
    id: Option<&str>,
    version: Option<i64>,
) -> Result<Option<(UnitDefinition, FrozenReference)>, AppError> {
    validate_unit_pair(id, version)?;
    let Some(id) = id else {
        return Ok(None);
    };
    let repo = repo.ok_or_else(|| {
        AppError::Forbidden("指标绑定了单位标准，但真实发布修订仓储未配置".into())
    })?;
    let revision = standards::active_revision(
        repo,
        tenant,
        "unit_standard",
        id,
        version.ok_or_else(|| bad("单位修订版本缺失"))?,
    )
    .await?;
    let unit = definition::unit(&revision.properties)?;
    Ok(Some((
        unit,
        FrozenReference {
            kind: "unit_standard".into(),
            id: revision.id,
            version: revision.revision_version,
            code: revision.code,
            status: "published".into(),
            properties: revision.properties,
            references: vec![],
            publication_hash: Some(revision.definition_hash),
        },
    )))
}
pub(super) async fn model(
    repo: Option<&dyn StandardGovernanceRepo>,
    tenant: &str,
    model: &ProductResource,
    publication: &ProductResource,
) -> Result<(), AppError> {
    standards::model_dependencies(repo, tenant, model, publication)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unit_reference_never_selects_the_latest_draft_implicitly() {
        assert!(validate_unit_pair(Some("unit"), None).is_err());
        assert!(validate_unit_pair(None, Some(2)).is_err());
        assert!(validate_unit_pair(Some("unit"), Some(0)).is_err());
        assert!(validate_unit_pair(Some("unit"), Some(2)).is_ok());
    }
    #[tokio::test]
    async fn configured_unit_requires_the_real_revision_repository() {
        assert!(matches!(
            unit(None, "tenant", Some("unit"), Some(2)).await,
            Err(AppError::Forbidden(_))
        ));
        assert!(unit(None, "tenant", None, None).await.unwrap().is_none());
    }
}
