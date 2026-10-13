use super::{AuthorizedQuery, QueryAuthorizationService, denied};
use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::metrics,
};
use chrono::Utc;
use dts_backend::repo::{
    data_access::{AccessOutcome, DataAccessEvidence, RecordDataAccessParams, validation},
    product_job::*,
};

#[derive(Clone)]
pub(super) struct MetricQueryScope {
    job_id: String,
    node_id: String,
    worker_id: String,
    fence: i64,
    source_version: i64,
    result_read: bool,
}
impl QueryAuthorizationService {
    pub(crate) async fn authorize_metric(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || job.actor_id != actor.sub
            || !metrics::is_calculable(&job.source_kind)
            || job.action != "execute_metric"
        {
            return Err(denied("指标查询的实际租户、执行人或来源不匹配"));
        }
        let scope = MetricQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: job
                .lease_owner
                .clone()
                .ok_or_else(|| denied("指标查询缺少真实租约"))?,
            fence: job.lease_fence,
            source_version: job.source_version,
            result_read: false,
        };
        self.authorize_metric_checked(actor, &scope, sql).await
    }
    pub(crate) async fn authorize_metric_result(
        &self,
        actor: &AuthIdentity,
        job: &ProductJob,
        node_id: &str,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        if job.tenant_id != actor.tenant_id
            || !metrics::is_calculable(&job.source_kind)
            || job.action != "execute_metric"
        {
            return Err(denied("指标历史结果的实际租户或来源不匹配"));
        }
        let scope = MetricQueryScope {
            job_id: job.id.clone(),
            node_id: node_id.into(),
            worker_id: String::new(),
            fence: 0,
            source_version: job.source_version,
            result_read: true,
        };
        self.authorize_metric_checked(actor, &scope, sql).await
    }
    async fn authorize_metric_checked(
        &self,
        actor: &AuthIdentity,
        scope: &MetricQueryScope,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        match self.authorize_metric_scope(actor, scope, sql).await {
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
    pub(super) async fn authorize_metric_scope(
        &self,
        actor: &AuthIdentity,
        scope: &MetricQueryScope,
        sql: &str,
    ) -> Result<AuthorizedQuery, AppError> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| denied("指标授权未绑定真实产品依赖仓储"))?;
        let jobs = self
            .jobs
            .as_ref()
            .ok_or_else(|| denied("指标授权未绑定真实任务仓储"))?;
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
            .ok_or_else(|| denied("指标任务不存在于当前租户"))?;
        if job.tenant_id != actor.tenant_id || job.source_version != scope.source_version {
            return Err(denied("指标任务身份或版本不匹配"));
        }
        if scope.result_read {
            if !matches!(
                job.status,
                ProductJobStatus::Succeeded
                    | ProductJobStatus::Failed
                    | ProductJobStatus::Cancelled
                    | ProductJobStatus::ReconciliationRequired
            ) {
                return Err(denied("历史指标结果入口仅允许已结束的真实任务"));
            }
        } else if job.actor_id != actor.sub
            || job.status != ProductJobStatus::Running
            || job.cancel_requested
            || job.lease_owner.as_deref() != Some(scope.worker_id.as_str())
            || job.lease_fence != scope.fence
            || scope.fence < 1
            || job
                .lease_expires_at
                .is_none_or(|expiry| expiry <= Utc::now())
        {
            return Err(denied("指标租约、取消状态或执行人已经失效"));
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
            return Err(denied("指标节点不处于可执行或可读取状态"));
        }
        let compiled = metrics::prepare_claimed(&job)?;
        if !compiled
            .iter()
            .any(|(id, compiled)| id == &scope.node_id && compiled == sql)
        {
            return Err(denied("SQL不符合实际指标任务固定编译器"));
        }
        metrics::verify_current(
            resources.as_ref(),
            self.metadata.as_ref(),
            self.standards.as_deref(),
            &job,
        )
        .await?;
        // Fixed metric SQL still passes the normal column grant parser. This
        // scope only proves the publication/job lifetime; it grants no SQL bypass.
        let mut query = self.authorize(actor, sql).await?;
        if metrics::requires_full_table(&job)? {
            let snapshot: metrics::MetricSnapshot = serde_json::from_value(job.definition_snapshot)
                .map_err(|_| denied("指标快照无效"))?;
            let full = self
                .authorize(actor, &metrics::full_count_sql(&snapshot)?)
                .await?;
            if full.targets != query.targets || full.evidence.all_data != query.evidence.all_data {
                return Err(denied("COUNT全字段授权与当前指标表不一致"));
            }
            query.evidence.columns = full.evidence.columns;
            query.evidence.grant_ids = full.evidence.grant_ids;
        }
        query.metrics = Some(scope.clone());
        Ok(query)
    }
}
