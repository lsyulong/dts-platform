use super::{
    ast, bad, denied, metrics::MetricQueryScope, quality::QualityQueryScope,
    recognition::RecognitionQueryScope,
};
use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::query_svc::validate_read_only_sql,
};
use chrono::Utc;
use dts_backend::repo::{data_access::*, metadata_catalog::MetadataCatalogRepo};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
pub struct QueryAuthorizationService {
    pub(super) repo: Arc<dyn DataAccessRepo>,
    pub(super) metadata: Arc<dyn MetadataCatalogRepo>,
    pub(super) access: QueryAccess,
    pub(super) jobs: Option<Arc<dyn dts_backend::repo::product_job::ProductJobRepo>>,
    pub(super) resources: Option<Arc<dyn dts_backend::ProductResourceRepo>>,
    pub(super) standards:
        Option<Arc<dyn dts_backend::repo::standard_governance::StandardGovernanceRepo>>,
}

#[derive(Clone)]
pub struct AuthorizedQuery {
    pub(super) sql: String,
    pub(super) tenant_id: String,
    pub(super) actor_id: String,
    pub(super) evidence: DataAccessEvidence,
    pub(super) targets: Vec<DataAccessTarget>,
    pub(super) quality: Option<QualityQueryScope>,
    pub(super) recognition: Option<RecognitionQueryScope>,
    pub(super) metrics: Option<MetricQueryScope>,
}

impl AuthorizedQuery {
    pub fn sql(&self) -> &str {
        &self.sql
    }
    pub fn evidence(&self) -> &DataAccessEvidence {
        &self.evidence
    }
    pub(crate) fn actor_scope(&self) -> (&str, &str) {
        (&self.tenant_id, &self.actor_id)
    }
    pub(crate) fn is_controlled_quality_count(&self) -> bool {
        self.quality.is_some()
    }
    pub(crate) fn is_controlled_sensitive_scan(&self) -> bool {
        self.recognition.is_some() && self.quality.is_none() && self.metrics.is_none()
    }
    #[cfg(test)]
    pub(crate) fn masking_test_token(actor: &AuthIdentity, sql: &str) -> Self {
        Self {
            sql: sql.into(),
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            evidence: DataAccessEvidence {
                query_hash: validation::query_hash(sql),
                ..Default::default()
            },
            targets: vec![],
            quality: None,
            recognition: None,
            metrics: None,
        }
    }
}

impl QueryAuthorizationService {
    pub fn new(
        repo: Arc<dyn DataAccessRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        access: QueryAccess,
    ) -> Self {
        Self {
            repo,
            metadata,
            access,
            jobs: None,
            resources: None,
            standards: None,
        }
    }

    pub fn with_resource_repo(
        mut self,
        resources: Arc<dyn dts_backend::ProductResourceRepo>,
    ) -> Self {
        self.resources = Some(resources);
        self
    }
    pub fn with_standard_repo(
        mut self,
        repo: Arc<dyn dts_backend::repo::standard_governance::StandardGovernanceRepo>,
    ) -> Self {
        self.standards = Some(repo);
        self
    }

    pub async fn authorize(
        &self,
        actor: &AuthIdentity,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        match self.authorize_inner(actor, sql).await {
            Ok(query) => Ok(query),
            Err(error) => {
                self.repo
                    .record_access(&RecordDataAccessParams {
                        id: uuid::Uuid::new_v4().to_string(),
                        tenant_id: actor.tenant_id.clone(),
                        actor_id: actor.sub.clone(),
                        outcome: AccessOutcome::Denied,
                        evidence: DataAccessEvidence {
                            query_hash: validation::query_hash(sql),
                            ..Default::default()
                        },
                    })
                    .await?;
                Err(error)
            },
        }
    }

    async fn authorize_inner(
        &self,
        actor: &AuthIdentity,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(denied("当前租户未绑定受控查询引擎"));
        }
        if sql.is_empty() || sql.len() > 65536 {
            return Err(bad("查询长度须在1至65536字节"));
        }
        validate_read_only_sql(sql)?;
        let authority = self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?;
        if !authority.active {
            return Err(denied("当前租户用户已停用或不存在"));
        }
        let mut evidence = DataAccessEvidence {
            query_hash: validation::query_hash(sql),
            all_data: authority.all_data,
            ..Default::default()
        };
        if authority.all_data {
            return Ok(AuthorizedQuery {
                sql: sql.into(),
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                evidence,
                targets: vec![],
                quality: None,
                recognition: None,
                metrics: None,
            });
        }
        let parsed = ast::parse(sql)?;
        let now = Utc::now();
        let grants = self
            .repo
            .active_grants(&actor.tenant_id, &actor.sub, now)
            .await?;
        if grants.len() > 1000 {
            return Err(denied("有效授权超过查询容量，请缩小权限范围"));
        }
        let mut valid = Vec::new();
        let mut proofs = std::collections::BTreeMap::new();
        for grant in grants {
            if grant.tenant_id != actor.tenant_id
                || grant.grantee_id != actor.sub
                || grant.revoked_at.is_some()
                || grant.expires_at <= now
                || !ast::matches(&parsed, &grant.target)
            {
                continue;
            }
            validation::target(&grant.target, &actor.tenant_id, &grant.columns)?;
            let key = (
                grant.target.metadata_id.clone(),
                grant.target.binding.object_version,
            );
            if !proofs.contains_key(&key) {
                let proof = self
                    .metadata
                    .verified_binding(&actor.tenant_id, &key.0, key.1)
                    .await?;
                proofs.insert(key.clone(), proof);
            }
            if proofs.get(&key).and_then(Option::as_ref) == Some(&grant.target.binding) {
                valid.push(grant);
            }
        }
        let first = valid
            .first()
            .ok_or_else(|| denied("当前用户没有该真实数据表的有效授权"))?;
        if valid.iter().any(|grant| grant.target != first.target) {
            return Err(denied("同一查询表的冻结元数据身份存在冲突"));
        }
        let used = ast::columns(&parsed, &first.target)?;
        let allowed: BTreeSet<_> = valid
            .iter()
            .flat_map(|grant| grant.columns.iter().cloned())
            .collect();
        if !used.is_subset(&allowed) {
            return Err(denied("查询投影、条件或排序引用了未授权字段"));
        }
        evidence.grant_ids = valid.iter().map(|grant| grant.id.clone()).collect();
        evidence.grant_ids.sort();
        evidence.metadata_ids = vec![first.target.metadata_id.clone()];
        evidence.columns = used.into_iter().collect();
        Ok(AuthorizedQuery {
            sql: sql.into(),
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            evidence,
            targets: vec![first.target.clone()],
            quality: None,
            recognition: None,
            metrics: None,
        })
    }

    /// Recheck before engine dispatch and before releasing results. It cannot
    /// physically cancel a query already accepted by the separate engine.
    pub async fn reauthorize(
        &self,
        actor: &AuthIdentity,
        query: &AuthorizedQuery,
    ) -> Result<(), AppError> {
        self.identity(actor, query)?;
        let latest = match (&query.quality, &query.recognition, &query.metrics) {
            (Some(scope), None, None) => {
                self.authorize_quality_scope(actor, scope, &query.sql)
                    .await?
            },
            (None, Some(scope), None) => {
                self.authorize_recognition_scope(actor, scope, &query.sql)
                    .await?
            },
            (None, None, Some(scope)) => {
                self.authorize_metric_scope(actor, scope, &query.sql)
                    .await?
            },
            (None, None, None) => self.authorize_inner(actor, &query.sql).await?,
            _ => return Err(denied("受控查询领域身份存在冲突")),
        };
        if latest.targets != query.targets
            || latest.evidence.all_data != query.evidence.all_data
            || query
                .evidence
                .grant_ids
                .iter()
                .any(|id| !latest.evidence.grant_ids.contains(id))
        {
            return Err(denied("执行期间授权或元数据发生变化，不允许释放查询结果"));
        }
        Ok(())
    }

    pub async fn record_access(
        &self,
        actor: &AuthIdentity,
        query: &AuthorizedQuery,
        outcome: AccessOutcome,
    ) -> Result<(), AppError> {
        self.identity(actor, query)?;
        Ok(self
            .repo
            .record_access(&RecordDataAccessParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                outcome,
                evidence: query.evidence.clone(),
            })
            .await?)
    }

    fn identity(&self, actor: &AuthIdentity, query: &AuthorizedQuery) -> Result<(), AppError> {
        if actor.sub != query.actor_id || actor.tenant_id != query.tenant_id {
            Err(denied("授权查询不能转交其他用户或租户执行"))
        } else {
            Ok(())
        }
    }
}
