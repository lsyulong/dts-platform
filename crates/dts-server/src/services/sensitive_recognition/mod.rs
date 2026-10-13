// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0
mod compiler;
mod matcher;
#[cfg(test)]
mod tests;
mod types;
use crate::services::data_access::{AccessOutcome, QueryAuthorizationService};
use crate::{
    api::query::QueryAccess,
    models::{auth::AuthIdentity, error::AppError},
    services::{auth_svc::AuthService, product_jobs::validate_idempotency_key},
};
pub use compiler::prepare_claimed;
use dts_backend::{
    ProductResource, ProductResourceFilter, ProductResourceRepo, ResourceWrite,
    UpdateResourceParams,
    repo::{
        metadata_catalog::{MetadataCatalogRepo, binding_matches},
        product_job::*,
        sensitive_findings::*,
    },
};
pub(crate) use matcher::scan;
use serde_json::{Value, json};
use std::sync::Arc;
pub use types::*;

pub fn validate_definition(kind: &str, properties: &Value) -> Result<(), AppError> {
    if properties.get("publication").is_some() {
        return Err(bad("发布证明仅由服务端生成"));
    }
    match kind {
        "data_classification" => {
            compiler::classification(properties)?;
        },
        "recognition_rule" => {
            compiler::rule(properties)?;
        },
        "recognition_task" => {
            compiler::task(properties)?;
        },
        _ => return Err(bad("未知敏感识别定义类型")),
    }
    Ok(())
}

#[derive(Clone)]
pub struct SensitiveRecognitionService {
    resources: Arc<dyn ProductResourceRepo>,
    jobs: Arc<dyn ProductJobRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    findings: Arc<dyn SensitiveFindingRepo>,
    auth: AuthService,
    access: QueryAccess,
    query_authorization: Option<Arc<QueryAuthorizationService>>,
}
impl SensitiveRecognitionService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        jobs: Arc<dyn ProductJobRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        findings: Arc<dyn SensitiveFindingRepo>,
        auth: AuthService,
        access: QueryAccess,
    ) -> Self {
        Self {
            resources,
            jobs,
            metadata,
            findings,
            auth,
            access,
            query_authorization: None,
        }
    }
    pub fn with_query_authorization(self, service: Arc<QueryAuthorizationService>) -> Self {
        Self {
            query_authorization: Some(service),
            ..self
        }
    }
    async fn authorize(&self, actor: &AuthIdentity, action: &str) -> Result<(), AppError> {
        self.auth.validate_identity(actor).await?;
        self.auth.authorize(actor, "sensitive_data:read").await?;
        if action != "read" {
            self.auth
                .authorize(actor, &format!("sensitive_data:{action}"))
                .await?;
        }
        Ok(())
    }
    async fn resource(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: Option<i64>,
    ) -> Result<ProductResource, AppError> {
        let value = self
            .resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("敏感识别引用不属于当前租户或已不存在".into()))?;
        if version.is_some_and(|version| version != value.version) {
            return Err(AppError::Conflict("敏感识别定义版本已变化".into()));
        }
        Ok(value)
    }
    async fn classification(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        let value = self
            .resource(actor, "data_classification", id, None)
            .await?;
        compiler::classification(&value.properties)?;
        if value.status != "published" {
            return Err(AppError::Conflict("识别规则须引用已发布密级".into()));
        }
        Ok(value)
    }
    async fn snapshot(
        &self,
        actor: &AuthIdentity,
        source: &ProductResource,
    ) -> Result<RecognitionSnapshot, AppError> {
        if source.status == "published" {
            let frozen = &source.properties["publication"];
            if frozen["format_version"] != 1 {
                return Err(bad("已发布识别任务缺少服务端冻结来源版本"));
            }
            let rule: FrozenResource = serde_json::from_value(frozen["rule"].clone())
                .map_err(|_| bad("冻结识别规则无效"))?;
            let classification: FrozenResource =
                serde_json::from_value(frozen["classification"].clone())
                    .map_err(|_| bad("冻结密级无效"))?;
            let metadata: RecognitionMetadata = serde_json::from_value(frozen["metadata"].clone())
                .map_err(|_| bad("冻结识别来源无效"))?;
            for (kind, child) in [
                ("recognition_rule", &rule),
                ("data_classification", &classification),
            ] {
                let current = self
                    .resource(actor, kind, &child.id, Some(child.version))
                    .await?;
                if current.status != "published" || current.properties != child.properties {
                    return Err(AppError::Conflict("识别依赖定义已撤回或变化".into()));
                }
            }
            if self
                .metadata
                .verified_binding(&actor.tenant_id, &metadata.id, metadata.version)
                .await?
                .as_ref()
                != Some(&metadata.binding)
            {
                return Err(AppError::Conflict("冻结识别来源证明已变化".into()));
            }
            let value = RecognitionSnapshot {
                format_version: 1,
                executor: "sensitive_scan_v1".into(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: source.kind.clone(),
                source_id: source.id.clone(),
                source_version: source.version,
                properties: source.properties.clone(),
                task: FrozenResource::from(source),
                rule,
                classification,
                metadata,
            };
            compiler::validate_snapshot(&value)?;
            return Ok(value);
        }
        let task = compiler::task(&source.properties)?;
        let rule = self
            .resource(actor, "recognition_rule", &task.rule_id, None)
            .await?;
        compiler::rule(&rule.properties)?;
        if rule.status != "published" {
            return Err(AppError::Conflict("识别任务须引用已发布规则".into()));
        }
        let class = self
            .classification(
                actor,
                &compiler::rule(&rule.properties)?.0.classification_id,
            )
            .await?;
        if rule.properties["publication"]["classification"] != json!(FrozenResource::from(&class)) {
            return Err(AppError::Conflict("规则发布时冻结的密级已改变".into()));
        }
        let metadata = self
            .resource(actor, "metadata_object", &task.object_id, None)
            .await?;
        let binding = self
            .metadata
            .verified_binding(&actor.tenant_id, &metadata.id, metadata.version)
            .await?
            .filter(|v| {
                v.engine == "swanlake"
                    && v.tenant_id == actor.tenant_id
                    && binding_matches(v, &metadata.properties)
            })
            .ok_or_else(|| bad("识别对象没有当前实际内部采集证明"))?;
        let columns = serde_json::from_value(metadata.properties["columns"].clone())
            .map_err(|_| bad("实际字段快照无效"))?;
        let value = RecognitionSnapshot {
            format_version: 1,
            executor: "sensitive_scan_v1".into(),
            tenant_id: actor.tenant_id.clone(),
            source_kind: source.kind.clone(),
            source_id: source.id.clone(),
            source_version: source.version,
            properties: source.properties.clone(),
            task: FrozenResource::from(source),
            rule: FrozenResource::from(&rule),
            classification: FrozenResource::from(&class),
            metadata: RecognitionMetadata {
                id: metadata.id,
                version: metadata.version,
                properties: metadata.properties,
                binding,
                columns,
            },
        };
        compiler::validate_snapshot(&value)?;
        Ok(value)
    }
    pub async fn validate_saved(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        self.authorize(actor, "read").await?;
        let source = self.resource(actor, kind, id, Some(version)).await?;
        match kind {
            "data_classification" => {
                let definition = compiler::classification(&source.properties)?;
                let mut offset = 0;
                loop {
                    let page = self
                        .resources
                        .list(&ProductResourceFilter {
                            tenant_id: actor.tenant_id.clone(),
                            kind: kind.into(),
                            q: None,
                            status: None,
                            created_by: None,
                            limit: 100,
                            offset,
                        })
                        .await?;
                    if page.total > 1000 {
                        return Err(bad("密级目录超过校验容量"));
                    }
                    if page.data.iter().any(|v| {
                        v.id != source.id
                            && v.properties["rank"].as_u64() == Some(u64::from(definition.rank))
                    }) {
                        return Err(AppError::Conflict(
                            "密级顺序已被当前租户的另一个等级使用".into(),
                        ));
                    }
                    offset += page.data.len() as u32;
                    if i64::from(offset) >= page.total {
                        break;
                    }
                    if page.data.is_empty() {
                        return Err(AppError::Conflict("密级目录在校验期间改变".into()));
                    }
                }
            },
            "recognition_rule" => {
                let (rule, _) = compiler::rule(&source.properties)?;
                self.classification(actor, &rule.classification_id).await?;
            },
            "recognition_task" => {
                self.snapshot(actor, &source).await?;
            },
            _ => return Err(bad("未知敏感识别定义类型")),
        }
        Ok(
            json!({"valid":true,"executed":false,"actual_sample_required":kind=="recognition_task","classification_enforced":false}),
        )
    }
    pub async fn publish(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        self.authorize(actor, "write").await?;
        self.validate_saved(actor, kind, id, version).await?;
        let source = self.resource(actor, kind, id, Some(version)).await?;
        if !matches!(source.status.as_str(), "draft" | "inactive") {
            return Err(AppError::Conflict("当前敏感定义不能发布".into()));
        }
        let mut properties = source.properties.clone();
        match kind {
            "recognition_rule" => {
                let (rule, _) = compiler::rule(&properties)?;
                let class = self.classification(actor, &rule.classification_id).await?;
                properties["publication"] =
                    json!({"format_version":1,"classification":FrozenResource::from(&class)});
            },
            "recognition_task" => {
                let frozen = self.snapshot(actor, &source).await?;
                properties["publication"] = json!({"format_version":1,"rule":frozen.rule,"classification":frozen.classification,"metadata":frozen.metadata});
            },
            _ => {},
        }
        let updated = self
            .write(
                actor,
                ProductResource {
                    properties,
                    ..source
                },
                "published",
            )
            .await?;
        Ok(json!({"resource":updated,"executed":false,"classification_enforced":false}))
    }
    pub async fn withdraw(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<Value, AppError> {
        self.authorize(actor, "write").await?;
        let source = self.resource(actor, kind, id, Some(version)).await?;
        if source.status != "published" {
            return Err(AppError::Conflict("只能撤回已发布敏感定义".into()));
        }
        let mut properties = source.properties.clone();
        if let Some(object) = properties.as_object_mut() {
            object.remove("publication");
        }
        let updated = self
            .write(
                actor,
                ProductResource {
                    properties,
                    ..source
                },
                "inactive",
            )
            .await?;
        Ok(json!({"resource":updated,"withdrawn":true}))
    }
    async fn write(
        &self,
        actor: &AuthIdentity,
        source: ProductResource,
        status: &str,
    ) -> Result<ProductResource, AppError> {
        Ok(self
            .resources
            .update(
                &actor.tenant_id,
                &source.kind,
                &source.id,
                &UpdateResourceParams {
                    expected_version: source.version,
                    actor_id: actor.sub.clone(),
                    fields: ResourceWrite {
                        code: source.code,
                        name: source.name,
                        description: source.description,
                        parent_id: source.parent_id,
                        status: status.into(),
                        properties: source.properties,
                        references: source.references,
                    },
                },
            )
            .await?)
    }
    pub async fn enqueue(
        &self,
        actor: &AuthIdentity,
        id: &str,
        request: RecognitionRunRequest,
    ) -> Result<EnqueuedProductJob, AppError> {
        self.authorize(actor, "execute").await?;
        if self.access.tenant_id.as_deref() != Some(actor.tenant_id.as_str()) {
            return Err(AppError::Forbidden("当前租户未绑定敏感识别引擎".into()));
        }
        validate_idempotency_key(&request.idempotency_key)?;
        let source = self
            .resource(
                actor,
                "recognition_task",
                id,
                Some(request.expected_version),
            )
            .await?;
        if source.status != "published" {
            return Err(AppError::Conflict(
                "识别任务须先发布，撤回任务不能运行".into(),
            ));
        }
        let snapshot = self.snapshot(actor, &source).await?;
        let node = compiler::node(&snapshot)?;
        Ok(self
            .jobs
            .enqueue(&EnqueueProductJobParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                source_kind: "recognition_task".into(),
                source_id: id.into(),
                source_version: source.version,
                actor_id: actor.sub.clone(),
                action: "execute_recognition".into(),
                idempotency_key: request.idempotency_key,
                definition_snapshot: json!(snapshot),
                parameters: json!({}),
                can_retry: true,
                nodes: vec![node],
            })
            .await?)
    }
    pub async fn findings(
        &self,
        actor: &AuthIdentity,
        query: RecognitionListQuery,
    ) -> Result<SensitiveFindingPage, AppError> {
        self.authorize(actor, "read").await?;
        if self.query_authorization.is_none() {
            return Err(AppError::Forbidden(
                "敏感证据读取未配置当前数据授权守卫".into(),
            ));
        }
        let page = self
            .findings
            .list(&SensitiveFindingFilter {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                task_id: query.task_id,
                status: query.status,
                limit: query.limit.unwrap_or(20),
                offset: query.offset.unwrap_or(0),
            })
            .await?;
        for finding in &page.data {
            self.authorize_finding(actor, finding).await?;
        }
        Ok(page)
    }
    pub async fn finding(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<SensitiveFinding, AppError> {
        self.authorize(actor, "read").await?;
        let finding = self
            .findings
            .get(&actor.tenant_id, &actor.sub, id)
            .await?
            .ok_or_else(|| AppError::NotFound("可信敏感发现不存在".into()))?;
        self.authorize_finding(actor, &finding).await?;
        Ok(finding)
    }
    pub async fn review(
        &self,
        actor: &AuthIdentity,
        id: &str,
        confirm: bool,
        request: RecognitionReviewRequest,
    ) -> Result<Value, AppError> {
        self.authorize(actor, "write").await?;
        let finding = self
            .findings
            .review(&SensitiveReviewParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                finding_id: id.into(),
                expected_version: request.expected_version,
                confirm,
                reason: request.reason,
            })
            .await?;
        Ok(
            json!({"id":finding.id,"status":finding.status,"version":finding.version,"reviewed_by":finding.reviewed_by,"review_reason":finding.review_reason,"reviewed_at":finding.reviewed_at,"classification_enforced":false}),
        )
    }
    async fn authorize_finding(
        &self,
        actor: &AuthIdentity,
        finding: &SensitiveFinding,
    ) -> Result<(), AppError> {
        let authorization = self
            .query_authorization
            .as_ref()
            .ok_or_else(|| AppError::Forbidden("敏感证据读取未配置当前数据授权守卫".into()))?;
        let job = self
            .jobs
            .get(&ProductJobLookup {
                tenant_id: actor.tenant_id.clone(),
                job_id: finding.job_id.clone(),
                actor_id: None,
            })
            .await?
            .ok_or_else(|| AppError::NotFound("发现实际来源任务已不存在".into()))?;
        let sql = prepare_claimed(&job)?
            .into_iter()
            .find(|(id, _)| id == &finding.node_id)
            .ok_or_else(|| bad("发现的真实节点与冻结定义不匹配"))?
            .1;
        let query = authorization
            .authorize_recognition_result(actor, &job, &finding.node_id, &sql)
            .await?;
        if let Err(error) = authorization.reauthorize(actor, &query).await {
            authorization
                .record_access(actor, &query, AccessOutcome::Denied)
                .await?;
            return Err(error);
        }
        authorization
            .record_access(actor, &query, AccessOutcome::Succeeded)
            .await?;
        if let Err(error) = authorization.reauthorize(actor, &query).await {
            authorization
                .record_access(actor, &query, AccessOutcome::Denied)
                .await?;
            return Err(error);
        }
        Ok(())
    }
}
pub async fn verify_current(
    resources: &dyn ProductResourceRepo,
    metadata: &dyn MetadataCatalogRepo,
    job: &ProductJob,
) -> Result<(), AppError> {
    prepare_claimed(job)?;
    let snapshot: RecognitionSnapshot =
        serde_json::from_value(job.definition_snapshot.clone()).map_err(|_| bad("识别快照无效"))?;
    for (kind, frozen) in [
        ("recognition_task", &snapshot.task),
        ("recognition_rule", &snapshot.rule),
        ("data_classification", &snapshot.classification),
    ] {
        let current = resources
            .get(&job.tenant_id, kind, &frozen.id)
            .await?
            .filter(|v| {
                v.status == "published"
                    && v.version == frozen.version
                    && v.properties == frozen.properties
            })
            .ok_or_else(|| AppError::Conflict("识别任务、规则或密级已撤回或变化".into()))?;
        if current.tenant_id != job.tenant_id {
            return Err(AppError::Forbidden("识别定义租户不匹配".into()));
        }
    }
    if metadata
        .verified_binding(
            &job.tenant_id,
            &snapshot.metadata.id,
            snapshot.metadata.version,
        )
        .await?
        .as_ref()
        != Some(&snapshot.metadata.binding)
    {
        return Err(AppError::Conflict("识别来源内部证明已失效".into()));
    }
    Ok(())
}
fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
