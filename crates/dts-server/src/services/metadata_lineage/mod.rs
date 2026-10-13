use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::product::{ResourceAction, ResourceInput, ResourceUpdate},
};
use chrono::Utc;
use dts_backend::{
    CreateResourceParams, ProductResource, ProductResourceRepo, ResourceReference, ResourceWrite,
    UpdateResourceParams,
    repo::{metadata_catalog::MetadataCatalogRepo, metadata_lineage::*},
};
use serde_json::{Value, json};
use std::sync::Arc;

mod traversal;
pub use traversal::{LineageAnalysis, LineageAnalysisRequest, LineagePath};

#[derive(Clone)]
pub struct MetadataLineageService {
    resources: Arc<dyn ProductResourceRepo>,
    metadata: Arc<dyn MetadataCatalogRepo>,
    graph: Arc<dyn MetadataLineageRepo>,
}
impl MetadataLineageService {
    pub fn new(
        resources: Arc<dyn ProductResourceRepo>,
        metadata: Arc<dyn MetadataCatalogRepo>,
        graph: Arc<dyn MetadataLineageRepo>,
    ) -> Self {
        Self {
            resources,
            metadata,
            graph,
        }
    }
    pub async fn create(
        &self,
        actor: &AuthIdentity,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        let fields = self.fields(actor, input).await?;
        Ok(self
            .resources
            .create(&CreateResourceParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: actor.tenant_id.clone(),
                kind: "metadata_lineage".into(),
                actor_id: actor.sub.clone(),
                fields,
            })
            .await?)
    }
    pub async fn update(
        &self,
        actor: &AuthIdentity,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let version = input.expected_version;
        let fields = self
            .fields(
                actor,
                ResourceInput {
                    code: input.code,
                    name: input.name,
                    description: input.description,
                    parent_id: input.parent_id,
                    properties: input.properties,
                },
            )
            .await?;
        Ok(self
            .resources
            .update(
                &actor.tenant_id,
                "metadata_lineage",
                id,
                &UpdateResourceParams {
                    expected_version: version,
                    actor_id: actor.sub.clone(),
                    fields,
                },
            )
            .await?)
    }
    async fn fields(
        &self,
        actor: &AuthIdentity,
        input: ResourceInput,
    ) -> Result<ResourceWrite, AppError> {
        let code = input.code.trim().to_owned();
        let name = input.name.trim().to_owned();
        if !identifier(&code)
            || name.is_empty()
            || name.chars().count() > 128
            || name.chars().any(char::is_control)
            || input.parent_id.is_some()
            || input
                .description
                .as_ref()
                .is_some_and(|value| value.len() > 8192 || value.contains('\0'))
        {
            return Err(bad("血缘编码、名称或说明无效"));
        }
        let Some(properties) = input.properties.as_object() else {
            return Err(bad("血缘属性必须是对象"));
        };
        if properties.keys().any(|key| {
            !["source_object_id", "target_object_id", "evidence", "origin"].contains(&key.as_str())
        }) {
            return Err(bad("血缘包含未知字段或客户端不能填写的证据元信息"));
        }
        if properties.get("origin").and_then(Value::as_str) != Some("manual") {
            return Err(bad(
                "本次仅支持人工确认血缘；SQL和采集器证据须由真实解析器生成",
            ));
        }
        let source = text(&input.properties, "source_object_id", 128)?;
        let target = text(&input.properties, "target_object_id", 128)?;
        let evidence = text(&input.properties, "evidence", 8192)?;
        if source == target {
            return Err(bad("血缘对象不能引用自身"));
        }
        let source_version = self.verified(actor, &source).await?;
        let target_version = self.verified(actor, &target).await?;
        Ok(ResourceWrite {
            code,
            name,
            description: input.description,
            parent_id: None,
            status: "draft".into(),
            properties: json!({"source_object_id":source,"target_object_id":target,"origin":"manual","evidence":evidence,
                "source_object_version":source_version,"target_object_version":target_version,"evidence_actor_id":actor.sub,"evidence_recorded_at":Utc::now()}),
            references: vec![
                reference("source_object_id", "metadata_object", &source),
                reference("target_object_id", "metadata_object", &target),
                reference("evidence_actor_id", "user", &actor.sub),
            ],
        })
    }
    async fn verified(&self, actor: &AuthIdentity, id: &str) -> Result<i64, AppError> {
        let resource = self
            .resources
            .get(&actor.tenant_id, "metadata_object", id)
            .await?
            .ok_or_else(|| AppError::NotFound("血缘元数据对象不存在或不属于当前租户".into()))?;
        if resource.status != "published"
            || self
                .metadata
                .verified_binding(&actor.tenant_id, id, resource.version)
                .await?
                .is_none()
        {
            return Err(bad("血缘对象缺少当前真实采集证明"));
        }
        Ok(resource.version)
    }
    pub async fn action(
        &self,
        actor: &AuthIdentity,
        id: &str,
        action: &str,
        input: ResourceAction,
    ) -> Result<Value, AppError> {
        if action != "analyze" {
            return Err(bad("当前血缘动作不支持"));
        }
        let resource = self
            .resources
            .get(&actor.tenant_id, "metadata_lineage", id)
            .await?
            .ok_or_else(|| AppError::NotFound("血缘关系不存在".into()))?;
        if resource.version != input.expected_version {
            return Err(AppError::Conflict("血缘定义已更新，请刷新".into()));
        }
        let mut request: LineageAnalysisRequest = if input.payload.is_null() {
            Default::default()
        } else {
            serde_json::from_value(input.payload).map_err(|_| bad("血缘分析参数无效"))?
        };
        request.validate()?;
        let root = request.root_object_id.clone().unwrap_or(text(
            &resource.properties,
            "target_object_id",
            128,
        )?);
        request.root_object_id = Some(root.clone());
        let snapshot = self
            .graph
            .snapshot(&LineageSnapshotParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                max_edges: MAX_LINEAGE_EDGES,
                root_object_id: Some(root),
            })
            .await?;
        if snapshot
            .edges
            .iter()
            .any(|edge| edge.id == resource.id && edge.version != resource.version)
        {
            return Err(AppError::Conflict("血缘定义在分析前已经更新".into()));
        }
        let result = traversal::analyze(snapshot, &request)?;
        Ok(json!({"resource":resource,"result":result}))
    }
}
fn bad(message: &str) -> AppError {
    AppError::BadRequest(message.into())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().enumerate().all(|(index, c)| {
            c == '_' || c.is_ascii_alphabetic() || index > 0 && c.is_ascii_digit()
        })
}
fn text(value: &Value, key: &str, max: usize) -> Result<String, AppError> {
    value[key]
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= max && !value.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| bad("血缘对象或来源证据不能为空且不能超出长度限制"))
}
fn reference(field: &str, kind: &str, id: &str) -> ResourceReference {
    ResourceReference {
        field: field.into(),
        target_kind: kind.into(),
        target_id: id.into(),
    }
}
