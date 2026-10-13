use std::sync::Arc;

use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceFilter, ProductResourceRepo,
    ResourcePage, ResourceReference, ResourceWrite, UpdateResourceParams,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::models::{auth::AuthIdentity, error::AppError};

pub mod catalog;
mod content_publishing;
mod events;
mod publishing;
pub(crate) mod references;
pub(crate) mod validation;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceInput {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub properties: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceUpdate {
    pub expected_version: i64,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
    pub properties: Value,
}

impl ResourceUpdate {
    fn into_input(self) -> ResourceInput {
        ResourceInput {
            code: self.code,
            name: self.name,
            description: self.description,
            parent_id: self.parent_id,
            properties: self.properties,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceAction {
    pub expected_version: i64,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Deserialize)]
pub struct ListFilters {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Clone)]
pub struct ProductService {
    repo: Arc<dyn ProductResourceRepo>,
    publisher: crate::services::model_publisher::ModelPublisher,
    analytical_tenant: Option<String>,
    organizations: Option<Arc<dyn dts_backend::repo::OrganizationMembersRepo>>,
    standards: Option<Arc<crate::services::standards::StandardGovernanceService>>,
}

impl ProductService {
    pub fn new(
        repo: Arc<dyn ProductResourceRepo>,
        query: crate::services::query_svc::QueryService,
        analytical_tenant: Option<String>,
    ) -> Self {
        Self {
            repo,
            publisher: crate::services::model_publisher::ModelPublisher::new(query),
            analytical_tenant,
            organizations: None,
            standards: None,
        }
    }

    pub fn with_standards(
        mut self,
        service: Arc<crate::services::standards::StandardGovernanceService>,
    ) -> Self {
        self.standards = Some(service);
        self
    }

    pub fn with_organizations(
        mut self,
        repo: Arc<dyn dts_backend::repo::OrganizationMembersRepo>,
    ) -> Self {
        self.organizations = Some(repo);
        self
    }

    async fn organization_projection(
        &self,
        actor: &AuthIdentity,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        use dts_backend::repo::organization_members::{OrganizationScope, ReadOrganizationParams};
        let repo = self
            .organizations
            .as_ref()
            .ok_or_else(|| AppError::Internal("组织成员服务未配置".into()))?;
        let details = repo
            .organization(&ReadOrganizationParams {
                scope: OrganizationScope {
                    tenant_id: actor.tenant_id.clone(),
                    actor_id: actor.sub.clone(),
                },
                organization_id: id.into(),
            })
            .await?;
        let mut record = details.organization;
        record.properties["member_count"] = json!(details.member_count);
        Ok(record)
    }

    pub fn permission(kind: &str, write: bool) -> Result<String, AppError> {
        let module = catalog::module(kind)?;
        Ok(format!(
            "{}:{}",
            catalog::scope(module),
            if write { "write" } else { "read" }
        ))
    }

    pub fn schemas() -> Result<Vec<catalog::ResourceSchema>, AppError> {
        Ok(catalog::modules()?
            .iter()
            .filter(|m| m.resource_kind.is_some())
            .map(catalog::schema)
            .collect())
    }

    pub async fn list(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        filters: ListFilters,
    ) -> Result<ResourcePage, AppError> {
        catalog::module(kind)?;
        if filters.limit.is_some_and(|limit| limit == 0 || limit > 100) {
            return Err(validation::bad("每页条数须为 1 到 100"));
        }
        if filters.q.as_ref().is_some_and(|q| q.len() > 256) {
            return Err(validation::bad("搜索词过长"));
        }
        if kind == "operation_event" {
            if filters.q.as_ref().is_some_and(|v| !v.is_empty())
                || filters.status.as_ref().is_some_and(|v| !v.is_empty())
            {
                return Err(validation::bad(
                    "操作事件检索暂支持分页，请清除对象或状态筛选",
                ));
            }
            let page = self
                .repo
                .list_audit(
                    &actor.tenant_id,
                    None,
                    None,
                    filters.limit.unwrap_or(20),
                    filters.offset.unwrap_or(0),
                )
                .await?;
            return Ok(ResourcePage { total:page.total, data:page.data.into_iter().map(|event| ProductResource {
                id:event.id.clone(),tenant_id:event.tenant_id,kind:kind.into(),code:format!("audit_{}",event.id.replace('-',"")),
                name:format!("{} · {}",event.operation,event.resource_kind),description:None,parent_id:None,status:"recorded".into(),
                properties:json!({"actor_id":event.actor_id,"resource_kind":event.resource_kind,"resource_id":event.resource_id,"action":event.operation,"changes":event.summary}),
                version:1,created_by:event.actor_id,created_at:event.created_at,updated_at:event.created_at,references:vec![],
            }).collect() });
        }
        let mut page = self
            .repo
            .list(&ProductResourceFilter {
                tenant_id: actor.tenant_id.clone(),
                kind: kind.into(),
                q: filters.q,
                status: filters.status,
                limit: filters.limit.unwrap_or(20),
                offset: filters.offset.unwrap_or(0),
                created_by: if matches!(kind, "data_access_request" | "watermark_trace") {
                    Some(actor.sub.clone())
                } else {
                    None
                },
            })
            .await?;
        if kind == "organization" {
            for record in &mut page.data {
                *record = self.organization_projection(actor, &record.id).await?;
            }
        }
        Ok(page)
    }

    pub async fn get(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
    ) -> Result<ProductResource, AppError> {
        catalog::module(kind)?;
        if kind == "organization" {
            return self.organization_projection(actor, id).await;
        }
        let record = self
            .repo
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("业务对象不存在".into()))?;
        if matches!(kind, "data_access_request" | "watermark_trace")
            && record.created_by != actor.sub
        {
            return Err(AppError::NotFound("业务对象不存在".into()));
        }
        Ok(record)
    }

    pub async fn create(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        ensure_action(kind, "create")?;
        let fields = self.fields(actor, kind, input, "draft").await?;
        Ok(self
            .repo
            .create(&CreateResourceParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                kind: kind.into(),
                actor_id: actor.sub.clone(),
                fields,
            })
            .await?)
    }

    pub async fn update(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        ensure_action(kind, "update")?;
        let before = self.get(actor, kind, id).await?;
        editable(&before)?;
        let expected_version = input.expected_version;
        let mut fields = self
            .fields(actor, kind, input.into_input(), "draft")
            .await?;
        if kind == "data_model" {
            if let Some(coverage) = before.properties.get("standard_coverage") {
                fields.properties["standard_coverage"] = coverage.clone();
            }
        }
        Ok(self
            .repo
            .update(
                &actor.tenant_id,
                kind,
                id,
                &UpdateResourceParams {
                    expected_version,
                    actor_id: actor.sub.clone(),
                    fields,
                },
            )
            .await?)
    }

    pub async fn delete(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<(), AppError> {
        ensure_action(kind, "delete")?;
        let before = self.get(actor, kind, id).await?;
        editable(&before)?;
        Ok(self
            .repo
            .delete(&actor.tenant_id, kind, id, version, &actor.sub)
            .await?)
    }

    async fn fields(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        input: ResourceInput,
        status: &str,
    ) -> Result<ResourceWrite, AppError> {
        let module = catalog::module(kind)?;
        let code = input.code.trim().to_owned();
        let name = input.name.trim().to_owned();
        if !validation::identifier(&code)
            || name.is_empty()
            || name.chars().count() > 128
            || name.chars().any(char::is_control)
        {
            return Err(validation::bad(
                "编码须为字母、数字或下划线且首位不能是数字；名称限 128 字",
            ));
        }
        if input.description.as_ref().is_some_and(|v| v.len() > 8192) {
            return Err(validation::bad("说明过长"));
        }
        validation::properties(module, &input.properties, input.parent_id.as_deref())?;
        crate::services::product_jobs::ProductJobService::validate_schedule_properties(
            kind,
            &input.properties,
        )?;
        crate::services::product_domain::warehouse::validate(kind, &input.properties)?;
        if crate::services::metrics::handles(kind) {
            crate::services::metrics::validate_definition(kind, &input.properties)?;
        }
        if matches!(
            kind,
            "data_classification" | "recognition_rule" | "recognition_task"
        ) {
            crate::services::sensitive_recognition::validate_definition(kind, &input.properties)?;
        }
        if kind == "quality_monitor" {
            crate::services::quality_alerts::validate_definition(&input.properties)?;
        }
        if kind == "development_config" {
            dts_backend::repo::runtime_configuration::validation::policy(&input.properties)?;
        }
        crate::services::product_domain::access::validate(kind, &input.properties)?;
        let mut references = references::extract(module, &input.properties)?;
        if kind == "composite_metric" {
            for id in validation::strings(&input.properties["metric_ids"], "metric_ids")? {
                let mut target = None;
                for target_kind in ["atomic_metric", "derived_metric", "composite_metric"] {
                    if self
                        .repo
                        .get(&actor.tenant_id, target_kind, &id)
                        .await?
                        .is_some()
                    {
                        target = Some(target_kind);
                        break;
                    }
                }
                let target_kind =
                    target.ok_or_else(|| validation::bad("依赖指标不存在或不属于当前租户"))?;
                references.push(ResourceReference {
                    field: "metric_ids".into(),
                    target_kind: target_kind.into(),
                    target_id: id,
                });
            }
        }
        Ok(ResourceWrite {
            code,
            name,
            description: input.description,
            parent_id: input.parent_id,
            properties: input.properties,
            status: status.into(),
            references,
        })
    }

    pub async fn audit(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
    ) -> Result<dts_backend::ResourceAuditPage, AppError> {
        self.get(actor, kind, id).await?;
        Ok(self
            .repo
            .list_audit(&actor.tenant_id, Some(kind), Some(id), 100, 0)
            .await?)
    }

    pub async fn action(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        action: &str,
        input: ResourceAction,
    ) -> Result<Value, AppError> {
        ensure_action(kind, action)?;
        let record = self.get(actor, kind, id).await?;
        if record.version != input.expected_version {
            return Err(AppError::Conflict("业务对象已更新，请刷新后重试".into()));
        }
        if kind == "data_model" && action == "publish" {
            return self
                .publish_model(actor, record, input.expected_version)
                .await;
        }
        if matches!(kind, "portal" | "announcement") && matches!(action, "publish" | "withdraw") {
            return self.publish_content(actor, record, action).await;
        }
        let mut payload = input.payload;
        if action == "convert" {
            let target_id = payload
                .get("target_id")
                .and_then(Value::as_str)
                .ok_or_else(|| validation::bad("请选择真实目标单位"))?;
            let target = self.get(actor, "unit_standard", target_id).await?;
            payload["target"] = target.properties;
        }
        let result = match crate::services::product_domain::warehouse::action(
            kind,
            &record.properties,
            action,
            &payload,
        )? {
            Some(result) => result,
            None => crate::services::product_domain::access::action(
                kind,
                &record.properties,
                action,
                &payload,
            )?
            .ok_or_else(|| validation::bad("当前业务动作尚不可执行"))?,
        };
        Ok(json!({"resource":record,"result":result}))
    }
}

fn ensure_action(kind: &str, action: &str) -> Result<(), AppError> {
    if catalog::schema(catalog::module(kind)?)
        .actions
        .contains(&action)
    {
        Ok(())
    } else {
        Err(AppError::BadRequest("当前模块不支持此操作".into()))
    }
}

fn editable(record: &ProductResource) -> Result<(), AppError> {
    if matches!(
        record.status.as_str(),
        "draft" | "publish_failed" | "inactive"
    ) {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "当前状态不能编辑或删除，须先完成撤回或恢复流程".into(),
        ))
    }
}
