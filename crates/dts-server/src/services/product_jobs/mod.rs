// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dts_backend::{
    ProductResource, ProductResourceRepo, ResourceWrite, UpdateResourceParams,
    repo::{
        metadata_catalog::MetadataCatalogRepo, product_job::*,
        sensitive_findings::SensitiveFindingRepo, standard_governance::StandardGovernanceRepo,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        auth_svc::AuthService,
        data_access::QueryAuthorizationService,
        data_masking::MaskingService,
        query_svc::{QueryResult, QueryService},
    },
};

mod definition;
mod lifecycle;
pub(crate) mod parameters;
mod query_guard;
mod runner;
mod runtime_configuration;
pub(crate) mod schedule;
mod stored_result;
#[cfg(test)]
mod tests;
mod views;

pub use definition::{DefinitionValidation, validate_definition};
pub use runner::ProductJobWorkerHandle;
pub use schedule::WorkflowScheduleDefinition;
pub use views::{EnqueuedProductJobView, ProductJobPageView, ProductJobView, ResultAccess};

const SOURCE_KINDS: [&str; 10] = [
    "sql_document",
    "development_workflow",
    "quality_task",
    "metadata_collection",
    "bi_report",
    "data_api",
    "recognition_task",
    "atomic_metric",
    "derived_metric",
    "composite_metric",
];
const LEASE_SECONDS: u32 = 30;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub expected_version: i64,
    pub idempotency_key: String,
    #[serde(default = "empty_parameters")]
    pub parameters: Value,
}
fn empty_parameters() -> Value {
    json!({})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryRequest {
    pub idempotency_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobListQuery {
    pub source_kind: Option<String>,
    pub source_id: Option<String>,
    pub status: Option<ProductJobStatus>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[async_trait::async_trait]
pub trait ReadOnlyQueryExecutor: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError>;
}

#[async_trait::async_trait]
impl ReadOnlyQueryExecutor for QueryService {
    async fn execute(&self, sql: &str) -> Result<QueryResult, AppError> {
        QueryService::execute(self, sql).await
    }
}

#[derive(Clone)]
pub struct ProductJobService {
    jobs: Arc<dyn ProductJobRepo>,
    resources: Arc<dyn ProductResourceRepo>,
    auth: AuthService,
    query: Arc<dyn ReadOnlyQueryExecutor>,
    access: QueryAccess,
    metadata: Option<Arc<dyn MetadataCatalogRepo>>,
    query_authorization: Option<Arc<QueryAuthorizationService>>,
    masking: Option<Arc<MaskingService>>,
    sensitive_findings: Option<Arc<dyn SensitiveFindingRepo>>,
    standards: Option<Arc<dyn StandardGovernanceRepo>>,
    runtime_configuration:
        Option<Arc<dyn dts_backend::repo::runtime_configuration::RuntimeConfigurationRepo>>,
}

impl ProductJobService {
    pub(crate) fn validate_schedule_properties(
        kind: &str,
        properties: &Value,
    ) -> Result<(), AppError> {
        if kind == "development_workflow" {
            schedule::validate(properties)?;
        }
        Ok(())
    }

    pub fn new(
        jobs: Arc<dyn ProductJobRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        auth: AuthService,
        query: QueryService,
        access: QueryAccess,
    ) -> Self {
        Self::with_executor(jobs, resources, auth, Arc::new(query), access)
    }

    pub fn with_executor(
        jobs: Arc<dyn ProductJobRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        auth: AuthService,
        query: Arc<dyn ReadOnlyQueryExecutor>,
        access: QueryAccess,
    ) -> Self {
        Self {
            jobs,
            resources,
            auth,
            query,
            access,
            metadata: None,
            query_authorization: None,
            masking: None,
            sensitive_findings: None,
            standards: None,
            runtime_configuration: None,
        }
    }

    pub fn with_metadata_repo(mut self, repo: Arc<dyn MetadataCatalogRepo>) -> Self {
        self.metadata = Some(repo);
        self
    }
    pub fn with_standard_repo(mut self, repo: Arc<dyn StandardGovernanceRepo>) -> Self {
        self.standards = Some(repo);
        self
    }

    pub fn with_query_authorization(mut self, service: Arc<QueryAuthorizationService>) -> Self {
        self.query_authorization = Some(service);
        self
    }

    pub fn with_masking(mut self, service: Arc<MaskingService>) -> Self {
        self.masking = Some(service);
        self
    }
    pub fn with_sensitive_findings_repo(self, repo: Arc<dyn SensitiveFindingRepo>) -> Self {
        Self {
            sensitive_findings: Some(repo),
            ..self
        }
    }

    async fn authorize_read(&self, actor: &AuthIdentity, kind: &str) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth
            .authorize(actor, &format!("{}:read", scope(kind)))
            .await
    }

    async fn authorize_execution_role(
        &self,
        actor: &AuthIdentity,
        kind: &str,
    ) -> Result<(), AppError> {
        self.authorize_read(actor, kind).await?;
        self.auth
            .authorize(actor, &format!("{}:execute", scope(kind)))
            .await
    }

    async fn authorize_execute(&self, actor: &AuthIdentity, kind: &str) -> Result<(), AppError> {
        self.authorize_execution_role(actor, kind).await?;
        match self.access.tenant_id.as_deref() {
            Some(tenant) if tenant == actor.tenant_id => Ok(()),
            Some(_) => Err(AppError::Forbidden("当前租户未绑定只读执行引擎".into())),
            None => Err(AppError::BadRequest(
                "只读执行引擎未启用或未绑定租户".into(),
            )),
        }
    }

    async fn source(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<ProductResource, AppError> {
        if !SOURCE_KINDS.contains(&kind) {
            return Err(AppError::NotFound("只读执行来源不存在".into()));
        }
        let source = self
            .resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("只读执行来源不存在".into()))?;
        if source.version != version {
            return Err(AppError::Conflict("定义版本已经改变，请刷新后操作".into()));
        }
        Ok(source)
    }

    pub async fn validate(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<DefinitionValidation, AppError> {
        self.authorize_read(actor, kind).await?;
        let source = self.source(actor, kind, id, version).await?;
        validate_definition(kind, &source.properties)
    }

    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        self.authorize_execution_role(actor, kind).await?;
        self.auth.authorize(actor, "development:write").await?;
        let source = self.source(actor, kind, id, version).await?;
        if !["draft", "inactive", "publish_failed"].contains(&source.status.as_str()) {
            return Err(AppError::Conflict("当前定义状态不能发布".into()));
        }
        validate_definition(kind, &source.properties)?;
        if let Some(repo) = self.runtime_configuration.as_ref() {
            use dts_backend::repo::runtime_configuration::*;
            let input = self
                .runtime_input(actor, &source, RuntimeSourceAccess::Publish)
                .await?;
            crate::services::runtime_configuration::merged_values(
                &source,
                input.configuration.as_ref(),
                &json!({}),
            )?;
            let updated = repo
                .publish_source(&PublishRuntimeSourceParams {
                    source: crate::services::runtime_configuration::source_lookup(
                        actor,
                        &source,
                        RuntimeSourceAccess::Publish,
                    ),
                    validation: ValidatedRuntimeSource::from_validated(&source)?,
                    evidence: input.evidence,
                })
                .await?;
            return Ok(
                json!({"resource":updated,"publication_scope":"validated_read_only_definition_with_runtime_configuration","schedule_registered":false,"executed":false}),
            );
        }
        if source
            .properties
            .get("configuration_id")
            .is_some_and(|v| !v.is_null())
        {
            return Err(AppError::BadRequest(
                "声明了运行配置但实际消费者未配置".into(),
            ));
        }
        let updated = self
            .resources
            .update(
                &actor.tenant_id,
                kind,
                id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: "published".into(),
                        properties: source.properties,
                        references: source.references,
                    },
                },
            )
            .await?;
        Ok(
            json!({"resource":updated,"publication_scope":"validated_read_only_definition","schedule_registered":false,"executed":false}),
        )
    }

    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        request: RunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        self.authorize_execute(actor, kind).await?;
        validate_idempotency_key(&request.idempotency_key)?;
        let source = self
            .source(actor, kind, id, request.expected_version)
            .await?;
        if source.status != "published" {
            return Err(AppError::Conflict("请先发布经过校验的只读定义".into()));
        }
        let (values, evidence) = self
            .runtime_values(actor, &source, &request.parameters)
            .await?;
        let prepared = definition::prepare_resource(&source, &values)?;
        let mut snapshot = definition::snapshot(&source);
        if let Some(evidence) = evidence {
            snapshot["runtime_configuration"] = json!(evidence);
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
                action: "execute_read_only".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: snapshot,
                parameters: prepared.parameters,
                can_retry: true,
                nodes: prepared.nodes,
            })
            .await?)
    }

    pub async fn list(
        &self,
        actor: &AuthIdentity,
        query: JobListQuery,
    ) -> Result<ProductJobPageView, AppError> {
        let kind = query
            .source_kind
            .unwrap_or_else(|| "development_workflow".into());
        if !SOURCE_KINDS.contains(&kind.as_str()) {
            return Err(parameters::bad("只读运行来源类型无效"));
        }
        self.authorize_read(actor, &kind).await?;
        let limit = query.limit.unwrap_or(10);
        if !(1..=50).contains(&limit) {
            return Err(parameters::bad("运行分页条数必须为 1 至 50"));
        }
        let page = self
            .jobs
            .list(&ProductJobFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: None,
                source_kind: Some(kind),
                source_id: query.source_id,
                status: query.status,
                limit,
                offset: query.offset.unwrap_or(0),
            })
            .await?;
        Ok(ProductJobPageView {
            data: page.data.into_iter().map(ProductJobView::summary).collect(),
            total: page.total,
        })
    }

    pub async fn get(&self, actor: &AuthIdentity, id: &str) -> Result<ProductJobView, AppError> {
        let job = self.get_raw(actor, id).await?;
        Ok(self.result_view(actor, job).await)
    }

    async fn get_raw(&self, actor: &AuthIdentity, id: &str) -> Result<ProductJob, AppError> {
        self.auth.validate_identity(actor).await?;
        let job = self
            .jobs
            .get(&ProductJobLookup {
                tenant_id: actor.tenant_id.clone(),
                job_id: id.into(),
                actor_id: None,
            })
            .await?
            .filter(|job| SOURCE_KINDS.contains(&job.source_kind.as_str()))
            .ok_or_else(|| AppError::NotFound("只读运行不存在".into()))?;
        self.authorize_read(actor, &job.source_kind).await?;
        Ok(job)
    }

    pub async fn cancel(&self, actor: &AuthIdentity, id: &str) -> Result<ProductJobView, AppError> {
        let job = self.get_raw(actor, id).await?;
        self.authorize_execution_role(actor, &job.source_kind)
            .await?;
        let updated = self
            .jobs
            .request_cancel(&CancelProductJobParams {
                tenant_id: actor.tenant_id.clone(),
                job_id: id.into(),
                actor_id: actor.sub.clone(),
            })
            .await?;
        Ok(self.result_view(actor, updated).await)
    }

    pub async fn retry(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: RetryRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        let job = self.get_raw(actor, id).await?;
        self.authorize_execute(actor, &job.source_kind).await?;
        if !job.can_retry
            || !matches!(
                job.status,
                ProductJobStatus::Failed | ProductJobStatus::Cancelled
            )
        {
            return Err(AppError::Conflict(
                "仅失败或取消的已校验只读运行可手动重试".into(),
            ));
        }
        runner::compile_claimed(&job)?;
        if job.source_kind == "recognition_task"
            || crate::services::metrics::is_calculable(&job.source_kind)
        {
            let metadata = self
                .metadata
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("当前运行来源未绑定实际元数据仓储".into()))?;
            if job.source_kind == "recognition_task" {
                crate::services::sensitive_recognition::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    &job,
                )
                .await?;
            } else {
                crate::services::metrics::verify_current(
                    self.resources.as_ref(),
                    metadata.as_ref(),
                    self.standards.as_deref(),
                    &job,
                )
                .await?;
            }
        }
        validate_idempotency_key(&request.idempotency_key)?;
        let source = self
            .source(actor, &job.source_kind, &job.source_id, job.source_version)
            .await?;
        self.verify_runtime(actor, &job, true).await?;
        if !executable_source(&source) || source.properties != job.definition_snapshot["properties"]
        {
            return Err(AppError::Conflict(
                "重试来源快照已改变或已撤回，请从当前定义重新运行".into(),
            ));
        }
        let nodes = job
            .nodes
            .iter()
            .map(|node| ProductJobNodeDefinition {
                id: node.id.clone(),
                name: node.name.clone(),
                definition: node.definition.clone(),
                depends_on: node.depends_on.clone(),
            })
            .collect();
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: job.source_kind,
                source_id: job.source_id,
                source_version: job.source_version,
                actor_id: actor.sub.clone(),
                action: job.action,
                idempotency_key: request.idempotency_key,
                definition_snapshot: job.definition_snapshot,
                parameters: job.parameters,
                can_retry: true,
                nodes,
            })
            .await?)
    }
}

fn scope(kind: &str) -> &'static str {
    if kind == "quality_task" {
        "quality"
    } else if kind == "metadata_collection" {
        "metadata"
    } else if kind == "bi_report" {
        "bi"
    } else if kind == "data_api" {
        "services"
    } else if kind == "recognition_task" {
        "sensitive_data"
    } else if crate::services::metrics::is_calculable(kind) {
        "metrics"
    } else {
        "development"
    }
}

fn executable_source(source: &ProductResource) -> bool {
    if ["quality_task", "metadata_collection"].contains(&source.kind.as_str()) {
        matches!(source.status.as_str(), "draft" | "published")
    } else {
        source.status == "published"
    }
}

pub(crate) fn validate_idempotency_key(key: &str) -> Result<(), AppError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
    {
        return Err(parameters::bad(
            "幂等键须为最多 128 字节的字母、数字或 _-.:",
        ));
    }
    Ok(())
}
