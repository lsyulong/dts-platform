use super::{AuthorizedQuery, QueryAuthorizationService, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::sensitive_recognition::{RecognitionSnapshot, prepare_claimed},
};
use chrono::Utc;
use dts_backend::repo::{
    data_access::{validation, *},
    metadata_catalog::{MetadataColumn, binding_matches},
    product_job::*,
};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct RecognitionQueryScope {
    job_id: String,
    node_id: String,
    worker_id: String,
    fence: i64,
    source_version: i64,
    result_read: bool,
}

impl QueryAuthorizationService {
    /// Available to the persisted recognition worker only, never a request-supplied
    /// trusted flag. It reconstructs the fixed compiler's exact SQL every time.
    pub(crate) async fn authorize_recognition(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || job.actor_id != actor.sub
            || job.source_kind != "recognition_task"
            || job.action != "execute_recognition"
        {
            return Err(denied("敏感识别查询的租户、执行人或来源不匹配"));
        }
        let scope = RecognitionQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: job
                .lease_owner
                .clone()
                .ok_or_else(|| denied("敏感识别查询缺少真实执行租约"))?,
            fence: job.lease_fence,
            source_version: job.source_version,
            result_read: false,
        };
        match self.authorize_recognition_scope(actor, &scope, sql).await {
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
    pub(crate) async fn authorize_recognition_result(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || job.source_kind != "recognition_task"
            || job.action != "execute_recognition"
        {
            return Err(denied("敏感识别结果的租户或来源不匹配"));
        }
        let scope = RecognitionQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: String::new(),
            fence: 0,
            source_version: job.source_version,
            result_read: true,
        };
        match self.authorize_recognition_scope(actor, &scope, sql).await {
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

    pub(super) async fn authorize_recognition_scope(
        &self,
        actor: &AuthIdentity,
        scope: &RecognitionQueryScope,
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
            return Err(denied("敏感识别执行用户不存在或已停用"));
        }
        let jobs = self
            .jobs
            .as_ref()
            .ok_or_else(|| denied("敏感识别查询授权尚未绑定真实任务仓储"))?;
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
            .ok_or_else(|| denied("敏感识别执行任务不存在"))?;
        if job.tenant_id != actor.tenant_id || job.source_version != scope.source_version {
            return Err(denied("敏感识别运行身份或版本不匹配"));
        }
        if scope.result_read {
            if job.status != ProductJobStatus::Succeeded {
                return Err(denied("历史结果入口只接受已结束的真实敏感识别运行"));
            }
        } else if job.actor_id != actor.sub
            || job.status != ProductJobStatus::Running
            || job.cancel_requested
            || job.lease_owner.as_deref() != Some(scope.worker_id.as_str())
            || job.lease_fence != scope.fence
            || scope.fence < 1
            || job.lease_expires_at.is_none_or(|expiry| expiry <= now)
        {
            return Err(denied("敏感识别执行租约、取消状态或执行身份已经失效"));
        }
        let compiled = prepare_claimed(&job)?;
        if !compiled
            .iter()
            .any(|(id, compiled)| id == &scope.node_id && compiled == sql)
        {
            return Err(denied("传入查询不是当前敏感识别节点固定编译器生成的SQL"));
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
            return Err(denied("敏感识别节点不处于可执行状态"));
        }
        if scope.result_read {
            let stored = job
                .nodes
                .iter()
                .find(|node| node.id == scope.node_id)
                .and_then(|node| node.result.clone())
                .ok_or_else(|| denied("实际敏感识别结果不存在"))?;
            let result: dts_backend::repo::sensitive_findings::SensitiveScanResult =
                serde_json::from_value(stored)
                    .map_err(|_| denied("历史识别结果不是无样本计数结构"))?;
            if !result.producer_committed
                || result.definition_hash
                    != dts_backend::repo::sensitive_findings::snapshot_hash(
                        &job.definition_snapshot,
                    )?
            {
                return Err(denied("历史识别结果缺少真实producer提交证明"));
            }
        }
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| denied("敏感识别授权没有绑定真实定义仓储"))?;
        crate::services::sensitive_recognition::verify_current(
            resources.as_ref(),
            self.metadata.as_ref(),
            &job,
        )
        .await?;
        let snapshot: RecognitionSnapshot = serde_json::from_value(job.definition_snapshot.clone())
            .map_err(|_| denied("敏感识别快照格式无效"))?;
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
            .ok_or_else(|| denied("敏感识别元数据没有当前真实采集证明"))?;
        let columns: Vec<MetadataColumn> =
            serde_json::from_value(snapshot.metadata.properties["columns"].clone())
                .map_err(|_| denied("敏感识别实际字段快照格式无效"))?;
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
                return Err(denied("受控敏感识别查询要求该真实表的完整字段授权"));
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
            quality: None,
            recognition: Some(scope.clone()),
            metrics: None,
        })
    }
}
