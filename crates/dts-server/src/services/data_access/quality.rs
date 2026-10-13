use super::{AuthorizedQuery, QueryAuthorizationService, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::data_quality::{QualitySnapshot, prepare_claimed},
};
use chrono::Utc;
use dts_backend::repo::{
    data_access::{validation, *},
    metadata_catalog::{MetadataColumn, binding_matches},
    product_job::*,
};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
pub(super) struct QualityQueryScope {
    job_id: String,
    node_id: String,
    worker_id: String,
    fence: i64,
    source_version: i64,
    result_read: bool,
}

impl QueryAuthorizationService {
    pub fn with_job_repo(mut self, jobs: Arc<dyn ProductJobRepo>) -> Self {
        self.jobs = Some(jobs);
        self
    }

    /// Available to the persisted quality worker only, never a request-supplied
    /// trusted flag. It reconstructs the fixed compiler's exact SQL every time.
    pub(crate) async fn authorize_quality(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || job.actor_id != actor.sub
            || job.source_kind != "quality_task"
            || job.action != "execute_quality"
        {
            return Err(denied("质量查询的租户、执行人或来源不匹配"));
        }
        let scope = QualityQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: job
                .lease_owner
                .clone()
                .ok_or_else(|| denied("质量查询缺少真实执行租约"))?,
            fence: job.lease_fence,
            source_version: job.source_version,
            result_read: false,
        };
        match self.authorize_quality_scope(actor, &scope, sql).await {
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

    /// Historical aggregate results require the current reader's own grants.
    /// A terminated job's executor identity is never the reader's entitlement.
    pub(crate) async fn authorize_quality_result(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || job.source_kind != "quality_task"
            || job.action != "execute_quality"
        {
            return Err(denied("质量结果的租户或来源不匹配"));
        }
        let scope = QualityQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: String::new(),
            fence: 0,
            source_version: job.source_version,
            result_read: true,
        };
        match self.authorize_quality_scope(actor, &scope, sql).await {
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

    pub(super) async fn authorize_quality_scope(
        &self,
        actor: &AuthIdentity,
        scope: &QualityQueryScope,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(denied("租户未绑定受控引擎"));
        }
        let now = Utc::now();
        let authority = self
            .repo
            .actor_authority(&actor.tenant_id, &actor.sub)
            .await?;
        if !authority.active {
            return Err(denied("质量执行用户不存在或已停用"));
        }
        let jobs = self
            .jobs
            .as_ref()
            .ok_or_else(|| denied("质量查询授权尚未绑定真实任务仓储"))?;
        let job = jobs
            .get(&ProductJobLookup {
                tenant_id: actor.tenant_id.clone(),
                job_id: scope.job_id.clone(),
                actor_id: if scope.result_read {
                    None
                } else {
                    Some(actor.sub.clone())
                },
            })
            .await?
            .ok_or_else(|| denied("质量执行任务不存在"))?;
        if job.tenant_id != actor.tenant_id || job.source_version != scope.source_version {
            return Err(denied("质量运行身份或版本不匹配"));
        }
        if scope.result_read {
            if !matches!(
                job.status,
                ProductJobStatus::Succeeded
                    | ProductJobStatus::Failed
                    | ProductJobStatus::Cancelled
                    | ProductJobStatus::ReconciliationRequired
            ) {
                return Err(denied("历史结果入口只接受已结束的真实质量运行"));
            }
        } else if job.actor_id != actor.sub
            || job.status != ProductJobStatus::Running
            || job.cancel_requested
            || job.lease_owner.as_deref() != Some(scope.worker_id.as_str())
            || job.lease_fence != scope.fence
            || scope.fence < 1
            || job.lease_expires_at.is_none_or(|expiry| expiry <= now)
        {
            return Err(denied("质量执行租约、取消状态或执行身份已经失效"));
        }
        let compiled = prepare_claimed(&job)?;
        if !compiled
            .iter()
            .any(|(id, compiled)| id == &scope.node_id && compiled == sql)
        {
            return Err(denied("传入查询不是当前质量节点固定编译器生成的SQL"));
        }
        if !job.nodes.iter().any(|node| {
            node.id == scope.node_id
                && if scope.result_read {
                    node.status == ProductJobNodeStatus::Succeeded && node.result.is_some()
                } else {
                    matches!(
                        node.status,
                        ProductJobNodeStatus::Pending | ProductJobNodeStatus::Running
                    )
                }
        }) {
            return Err(denied("质量节点不处于可执行状态"));
        }
        if scope.result_read {
            crate::services::data_quality::results_from_jobs(
                &actor.tenant_id,
                std::slice::from_ref(&job),
                1,
            )?;
        }
        let snapshot: QualitySnapshot = serde_json::from_value(job.definition_snapshot)
            .map_err(|_| denied("质量快照格式无效"))?;
        let binding = self
            .metadata
            .verified_binding(
                &actor.tenant_id,
                &snapshot.metadata.id,
                snapshot.metadata.version,
            )
            .await?
            .filter(|binding| {
                binding.engine == "swanlake"
                    && binding.tenant_id == actor.tenant_id
                    && binding.object_version == snapshot.metadata.version
                    && binding_matches(binding, &snapshot.metadata.properties)
            })
            .ok_or_else(|| denied("质量元数据没有当前真实采集证明"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(snapshot.metadata.properties["columns"].clone())
                .map_err(|_| denied("质量实际字段快照格式无效"))?;
        let target = DataAccessTarget {
            metadata_id: snapshot.metadata.id,
            binding,
            columns,
        };
        let required: BTreeSet<_> = target
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let mut evidence = DataAccessEvidence {
            query_hash: validation::query_hash(sql),
            metadata_ids: vec![target.metadata_id.clone()],
            columns: required.iter().cloned().collect(),
            all_data: authority.all_data,
            ..Default::default()
        };
        if !authority.all_data {
            let grants = self
                .repo
                .active_grants(&actor.tenant_id, &actor.sub, now)
                .await?;
            if grants.len() > 1000 {
                return Err(denied("有效授权超过执行容量"));
            }
            let grants: Vec<_> = grants
                .into_iter()
                .filter(|grant| {
                    grant.tenant_id == actor.tenant_id
                        && grant.grantee_id == actor.sub
                        && grant.revoked_at.is_none()
                        && grant.expires_at > now
                        && grant.target == target
                })
                .collect();
            let allowed: BTreeSet<_> = grants
                .iter()
                .flat_map(|grant| grant.columns.iter().cloned())
                .collect();
            if required.is_empty() || !required.is_subset(&allowed) {
                return Err(denied("受控质量COUNT查询要求该真实表的完整字段授权"));
            }
            evidence.grant_ids = grants.into_iter().map(|grant| grant.id).collect();
            evidence.grant_ids.sort();
        }
        Ok(AuthorizedQuery {
            sql: sql.into(),
            tenant_id: actor.tenant_id.clone(),
            actor_id: actor.sub.clone(),
            evidence,
            targets: vec![target],
            quality: Some(scope.clone()),
            recognition: None,
            metrics: None,
        })
    }
}
