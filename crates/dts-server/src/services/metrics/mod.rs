use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        auth_svc::AuthService, data_access::QueryAuthorizationService,
        product_jobs::validate_idempotency_key,
    },
};
use dts_backend::{
    ProductResource, ProductResourceRepo, ResourceReference, ResourceWrite, UpdateResourceParams,
    repo::{
        metadata_catalog::MetadataCatalogRepo, product_job::*,
        standard_governance::StandardGovernanceRepo,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

mod compiler;
mod definition;
mod expression;
mod freeze;
mod precision;
mod predicate;
mod proof;
mod result;
mod standard_dependencies;
#[cfg(test)]
mod tests;
mod types;
mod window;
pub use compiler::prepare_claimed;
pub(crate) use compiler::requires_full_table;
pub use definition::validate_definition;
pub use result::map_node_result;
pub use types::*;

pub fn is_calculable(kind: &str) -> bool {
    matches!(
        kind,
        "atomic_metric" | "derived_metric" | "composite_metric"
    )
}
pub fn handles(kind: &str) -> bool {
    is_calculable(kind) || matches!(kind, "metric_modifier" | "metric_period")
}
fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}

#[derive(Clone)]
pub struct MetricsService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    auth: AuthService,
    access: QueryAccess,
    query_authorization: Option<Arc<QueryAuthorizationService>>,
    standards: Option<Arc<dyn StandardGovernanceRepo>>,
}
impl MetricsService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        auth: AuthService,
        access: QueryAccess,
    ) -> Self {
        Self {
            resources,
            jobs,
            metadata,
            auth,
            access,
            query_authorization: None,
            standards: None,
        }
    }
    pub fn with_query_authorization(mut self, service: Arc<QueryAuthorizationService>) -> Self {
        self.query_authorization = Some(service);
        self
    }
    pub fn with_standard_repo(mut self, repo: Arc<dyn StandardGovernanceRepo>) -> Self {
        self.standards = Some(repo);
        self
    }
    async fn source(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<ProductResource, AppError> {
        if !is_calculable(kind) && !matches!(kind, "metric_modifier" | "metric_period") {
            return Err(bad("未知指标模块"));
        }
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "metrics:read").await?;
        let source = proof::get(self.resources.as_ref(), &actor.tenant_id, kind, id).await?;
        if source.version != version {
            return Err(AppError::Conflict("指标定义版本已经改变".into()));
        }
        Ok(source)
    }
    async fn freeze(
        &self,
        source: &ProductResource,
        version: i64,
    ) -> Result<MetricPublication, AppError> {
        freeze::Builder::new(
            self.resources.as_ref(),
            self.metadata.as_ref(),
            self.standards.as_deref(),
        )
        .build(source, version, BTreeSet::new())
        .await
    }
    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, kind, id, expected_version).await?;
        let frozen = self.freeze(&source, source.version).await?;
        Ok(
            json!({"valid":true,"binding_validated":frozen.metadata.is_some(),"plan":frozen.plan,"period":frozen.period,"modifier":frozen.modifier,
            "dependency_versions":frozen.references.iter().map(|reference|json!({"kind":reference.kind,"id":reference.id,"version":reference.version})).collect::<Vec<_>>(),"executed":false}),
        )
    }
    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, kind, id, expected_version).await?;
        self.auth.authorize(actor, "metrics:write").await?;
        if !matches!(source.status.as_str(), "draft" | "inactive") {
            return Err(AppError::Conflict(
                "当前指标状态不能发布，请先撤回或保存草稿".into(),
            ));
        }
        let version = source
            .version
            .checked_add(1)
            .ok_or_else(|| bad("指标版本溢出"))?;
        let publication = self.freeze(&source, version).await?;
        let mut properties = definition::definition_properties(&source.properties)?;
        properties["publication"] = json!(publication);
        let mut references = source.references.clone();
        references.retain(|reference| reference.field != "publication_dependency");
        for dependency in &publication.references {
            if dependency.kind == kind && dependency.id == id {
                return Err(bad("发布依赖不能引用自身"));
            }
            references.push(ResourceReference {
                field: "publication_dependency".into(),
                target_kind: dependency.kind.clone(),
                target_id: dependency.id.clone(),
            });
        }
        if let Some(metadata) = &publication.metadata {
            references.push(ResourceReference {
                field: "publication_dependency".into(),
                target_kind: "metadata_object".into(),
                target_id: metadata.id.clone(),
            });
        }
        references.sort_by(|a, b| {
            (&a.field, &a.target_kind, &a.target_id).cmp(&(&b.field, &b.target_kind, &b.target_id))
        });
        references.dedup();
        let updated = self
            .resources
            .update(
                &actor.tenant_id,
                kind,
                id,
                &UpdateResourceParams {
                    expected_version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: "published".into(),
                        properties,
                        references,
                    },
                },
            )
            .await?;
        Ok(
            json!({"resource":updated,"publication_scope":"frozen_metric_definition","executed":false}),
        )
    }
    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        expected_version: i64,
    ) -> Result<Value, AppError> {
        let source = self.source(actor, kind, id, expected_version).await?;
        self.auth.authorize(actor, "metrics:write").await?;
        if source.status != "published" {
            return Err(AppError::Conflict("仅已发布的指标定义可以撤回".into()));
        }
        let updated = self
            .resources
            .update(
                &actor.tenant_id,
                kind,
                id,
                &UpdateResourceParams {
                    expected_version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: "inactive".into(),
                        properties: source.properties,
                        references: source.references,
                    },
                },
            )
            .await?;
        Ok(
            json!({"resource":updated,"withdrawn":true,"running_queries_physically_cancelled":false}),
        )
    }
    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        request: MetricRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        if !is_calculable(kind) {
            return Err(bad("修饰词与周期不能独立执行指标计算"));
        }
        let source = self
            .source(actor, kind, id, request.expected_version)
            .await?;
        self.auth.authorize(actor, "metrics:execute").await?;
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(AppError::Forbidden("当前租户未绑定指标计算引擎".into()));
        }
        validate_idempotency_key(&request.idempotency_key)?;
        let publication = proof::publication(&source)?;
        if self.freeze(&source, source.version).await? != publication {
            return Err(AppError::Conflict(
                "指标依赖、物理模型或实际元数据发生变化，请重新发布".into(),
            ));
        }
        let metadata = publication
            .metadata
            .clone()
            .ok_or_else(|| bad("指标执行元数据缺失"))?;
        let window = window::resolve(
            publication
                .plan
                .as_ref()
                .ok_or_else(|| bad("指标执行计划缺失"))?,
            &request,
        )?;
        let snapshot = MetricSnapshot {
            format_version: 1,
            executor: "metric_aggregate_v1".into(),
            tenant_id: actor.tenant_id.clone(),
            source_kind: kind.into(),
            source_id: id.into(),
            source_version: source.version,
            properties: source.properties,
            metadata,
            publication,
            request: request.clone(),
            window,
        };
        let sql = compiler::sql(&snapshot)?;
        let authorization = self
            .query_authorization
            .as_ref()
            .ok_or_else(|| AppError::Forbidden("指标未配置真实查询授权出口".into()))?;
        let query = authorization.authorize(actor, &sql).await?;
        authorization.reauthorize(actor, &query).await?;
        // COUNT(*) retains its full-column entitlement even when a modifier
        // compiles it to COUNT(CASE ...). The worker repeats this exact check.
        if snapshot.publication.plan.as_ref().is_some_and(|plan| {
            plan.measures
                .iter()
                .any(|measure| measure.aggregate == "COUNT" && measure.argument.is_none())
        }) {
            authorization
                .authorize(actor, &full_count_sql(&snapshot)?)
                .await?;
        }
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: kind.into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "execute_metric".into(),
                idempotency_key: request.idempotency_key.clone(),
                definition_snapshot: json!(snapshot),
                parameters: json!(request),
                can_retry: true,
                nodes: vec![compiler::node(&snapshot)?],
            })
            .await?)
    }
}

pub(crate) fn full_count_sql(snapshot: &MetricSnapshot) -> Result<String, AppError> {
    let binding = &snapshot.metadata.binding;
    let table = [
        &binding.table_catalog,
        &binding.schema_name,
        &binding.table_name,
    ]
    .iter()
    .map(|name| definition::quote(name))
    .collect::<Result<Vec<_>, _>>()?
    .join(".");
    Ok(format!("SELECT COUNT(*) FROM {table} LIMIT 1"))
}
pub async fn verify_current(
    resources: &dyn ProductResourceRepo,
    metadata: &dyn MetadataCatalogRepo,
    standards: Option<&dyn StandardGovernanceRepo>,
    job: &ProductJob,
) -> Result<(), AppError> {
    let snapshot = compiler::snapshot(job)?;
    let resource = proof::get(resources, &job.tenant_id, &job.source_kind, &job.source_id).await?;
    if resource.status != "published"
        || resource.version != job.source_version
        || resource.properties != snapshot.properties
    {
        return Err(AppError::Forbidden("指标发布版本已改变或已撤回".into()));
    }
    let publication = freeze::Builder::new(resources, metadata, standards)
        .build(&resource, resource.version, BTreeSet::new())
        .await?;
    if publication != snapshot.publication {
        return Err(AppError::Forbidden(
            "指标依赖、实际元数据或物理发布证明已失效".into(),
        ));
    }
    Ok(())
}
