mod common;
mod compiler;
mod predicate;
mod proof;
mod read_authorization;
mod reporting;
mod snapshot;
mod types;

use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
};
use common::{bad, frozen, ids, text};
use dts_backend::{
    ProductResource, ProductResourceRepo,
    repo::{metadata_catalog::MetadataCatalogRepo, product_job::*},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub use compiler::{compile_node, map_node_result};
pub use reporting::{report_from_jobs, results_from_jobs};
pub use snapshot::prepare_claimed;
pub use types::*;

#[derive(Clone)]
pub struct QualityService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    access: QueryAccess,
    metadata: Option<Arc<dyn MetadataCatalogRepo>>,
    read_guard: Option<Arc<crate::services::data_access::QueryAuthorizationService>>,
}

impl QualityService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        access: QueryAccess,
    ) -> Self {
        Self {
            resources,
            jobs,
            access,
            metadata: None,
            read_guard: None,
        }
    }

    async fn resource(
        &self,
        tenant: &str,
        kind: &str,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        self.resources
            .get(tenant, kind, id)
            .await?
            .filter(|row| row.tenant_id == tenant && row.kind == kind && row.id == id)
            .ok_or_else(|| AppError::NotFound("质量定义或引用对象不存在于当前租户".into()))
    }

    fn engine(&self, tenant: &str) -> Result<(), AppError> {
        match self.access.tenant_id.as_deref() {
            Some(bound) if bound == tenant => Ok(()),
            Some(_) => Err(AppError::Forbidden("当前租户没有绑定受控分析引擎".into())),
            None => Err(bad("受控分析引擎未启用或未绑定租户")),
        }
    }

    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<QualityDefinitionValidation, AppError> {
        let source = self.resource(&actor.tenant_id, kind, id).await?;
        if source.version != expected_version {
            return Err(AppError::Conflict("质量定义版本已改变".into()));
        }
        let prepared = self.freeze(&actor.tenant_id, &source).await?;
        snapshot::validate_snapshot(&prepared, false)?;
        let nodes = snapshot::nodes(&prepared)?;
        let first = nodes
            .first()
            .map(|node| serde_json::from_value::<QualityNode>(node.definition.clone()))
            .transpose()
            .map_err(|_| bad("质量节点定义无效"))?;
        Ok(QualityDefinitionValidation {
            valid: true,
            rule_count: nodes.len(),
            table_name: first
                .map(|node| node.table_name)
                .unwrap_or(snapshot::table_name(&prepared.metadata.properties)?),
            engine_bound: self.verified_metadata(&prepared).await?,
            executed: false,
        })
    }

    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: QualityRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        self.engine(&actor.tenant_id)?;
        if request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 128
            || !request
                .idempotency_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
        {
            return Err(bad("幂等键必须是最多128字节的字母数字或 _-.:"));
        }
        let source = self.resource(&actor.tenant_id, "quality_task", id).await?;
        if source.version != request.expected_version
            || !["draft", "published"].contains(&source.status.as_str())
        {
            return Err(AppError::Conflict(
                "仅当前版本且未停用的质量任务可以执行".into(),
            ));
        }
        let frozen = self.freeze(&actor.tenant_id, &source).await?;
        snapshot::validate_snapshot(&frozen, true)?;
        if !self.verified_metadata(&frozen).await? {
            return Err(bad("元数据缺少当前租户的真实采集证明，不能执行质量查询"));
        }
        let nodes = snapshot::nodes(&frozen)?;
        let enqueued = self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: "quality_task".into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "execute_quality".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: serde_json::to_value(frozen)
                    .map_err(|_| bad("质量快照无法编码"))?,
                parameters: json!({}),
                can_retry: true,
                nodes,
            })
            .await?;
        if !enqueued.created {
            let tokens = self
                .authorize_results(actor, std::iter::once(&enqueued.job))
                .await?;
            self.finish_result_read(actor, &tokens).await?;
        }
        Ok(enqueued)
    }

    async fn freeze(
        &self,
        tenant: &str,
        source: &ProductResource,
    ) -> Result<QualitySnapshot, AppError> {
        let (object, rule_ids, task) = match source.kind.as_str() {
            "quality_task" => {
                let object = self
                    .resource(
                        tenant,
                        "quality_object",
                        text(&source.properties, "object_id")?,
                    )
                    .await?;
                let ids = ids(&source.properties, "rule_ids", true)?;
                (object, ids, source.clone())
            },
            "quality_rule" => {
                let object = self
                    .resource(
                        tenant,
                        "quality_object",
                        text(&source.properties, "object_id")?,
                    )
                    .await?;
                (object, vec![source.id.clone()], source.clone())
            },
            "quality_object" => (source.clone(), vec![], source.clone()),
            _ => return Err(bad("只支持质量对象、规则或任务定义校验")),
        };
        let metadata = self
            .resource(
                tenant,
                "metadata_object",
                text(&object.properties, "metadata_object_id")?,
            )
            .await?;
        let mut rules = Vec::with_capacity(rule_ids.len());
        for id in rule_ids {
            rules.push(frozen(&self.resource(tenant, "quality_rule", &id).await?));
        }
        Ok(QualitySnapshot {
            format_version: 1,
            executor: "quality_count_v1".into(),
            tenant_id: tenant.into(),
            source_kind: source.kind.clone(),
            source_id: source.id.clone(),
            source_version: source.version,
            properties: source.properties.clone(),
            task: frozen(&task),
            object: frozen(&object),
            metadata: frozen(&metadata),
            rules,
        })
    }

    pub async fn get_job(&self, actor: &AuthIdentity, id: &str) -> Result<ProductJob, AppError> {
        let job = self
            .jobs
            .get(&ProductJobLookup {
                tenant_id: actor.tenant_id.clone(),
                job_id: id.into(),
                actor_id: None,
            })
            .await?
            .filter(|job| {
                job.tenant_id == actor.tenant_id
                    && job.source_kind == "quality_task"
                    && job.action == "execute_quality"
            })
            .ok_or_else(|| AppError::NotFound("质量执行记录不存在".into()))?;
        let tokens = self.authorize_results(actor, std::iter::once(&job)).await?;
        self.finish_result_read(actor, &tokens).await?;
        Ok(job)
    }

    pub async fn results(
        &self,
        actor: &AuthIdentity,
        task_id: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<QualityResultPage, AppError> {
        let page = self
            .jobs
            .list(&ProductJobFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: None,
                source_kind: Some("quality_task".into()),
                source_id: task_id,
                status: None,
                limit: limit.clamp(1, 100),
                offset,
            })
            .await?;
        let tokens = self.authorize_results(actor, page.data.iter()).await?;
        let result = results_from_jobs(&actor.tenant_id, &page.data, page.total)?;
        self.finish_result_read(actor, &tokens).await?;
        Ok(result)
    }

    pub async fn report(
        &self,
        actor: &AuthIdentity,
        task_id: Option<String>,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<QualityReport, AppError> {
        let page = self.window_jobs(&actor.tenant_id, task_id.clone()).await?;
        let tokens = self
            .authorize_results(
                actor,
                page.data.iter().filter(|job| {
                    let time = job.finished_at.unwrap_or(job.updated_at);
                    time >= start && time < end
                }),
            )
            .await?;
        let result = report_from_jobs(
            &actor.tenant_id,
            &page.data,
            page.total,
            task_id,
            None,
            start,
            end,
            None,
        )?;
        self.finish_result_read(actor, &tokens).await?;
        Ok(result)
    }

    pub async fn evaluate_monitor(
        &self,
        actor: &AuthIdentity,
        id: &str,
        expected_version: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<QualityReport, AppError> {
        let monitor = self
            .resource(&actor.tenant_id, "quality_monitor", id)
            .await?;
        if monitor.version != expected_version {
            return Err(AppError::Conflict("监控配置版本已经变化".into()));
        }
        let object_id = text(&monitor.properties, "object_id")?.to_owned();
        self.resource(&actor.tenant_id, "quality_object", &object_id)
            .await?;
        let threshold = monitor.properties["failure_threshold"]
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| bad("失败比例阈值必须在0至1之间"))?;
        let hours = monitor.properties["window_hours"]
            .as_i64()
            .filter(|value| (1..=8760).contains(value))
            .ok_or_else(|| bad("监控窗口必须是1至8760小时整数"))?;
        let start = now
            .checked_sub_signed(chrono::Duration::hours(hours))
            .ok_or_else(|| bad("监控时间窗口越界"))?;
        let page = self.window_jobs(&actor.tenant_id, None).await?;
        let tokens = self
            .authorize_results(
                actor,
                page.data.iter().filter(|job| {
                    let time = job.finished_at.unwrap_or(job.updated_at);
                    time >= start
                        && time < now
                        && job.definition_snapshot["object"]["id"].as_str()
                            == Some(object_id.as_str())
                }),
            )
            .await?;
        let result = report_from_jobs(
            &actor.tenant_id,
            &page.data,
            page.total,
            None,
            Some(object_id),
            start,
            now,
            Some(threshold),
        )?;
        self.finish_result_read(actor, &tokens).await?;
        Ok(result)
    }

    async fn window_jobs(
        &self,
        tenant: &str,
        task_id: Option<String>,
    ) -> Result<ProductJobPage, AppError> {
        let mut data = Vec::new();
        let mut total = 0;
        let mut seen = std::collections::HashSet::new();
        for offset in (0..2000).step_by(100) {
            let page = self
                .jobs
                .list(&ProductJobFilter {
                    tenant_id: tenant.into(),
                    actor_id: None,
                    source_kind: Some("quality_task".into()),
                    source_id: task_id.clone(),
                    status: None,
                    limit: 100,
                    offset,
                })
                .await?;
            total = total.max(page.total);
            let empty = page.data.is_empty();
            data.extend(
                page.data
                    .into_iter()
                    .filter(|job| seen.insert(job.id.clone())),
            );
            if empty || data.len() as i64 >= total {
                break;
            }
        }
        Ok(ProductJobPage { data, total })
    }
}

#[cfg(test)]
mod tests;
