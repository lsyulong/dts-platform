use std::sync::Arc;

use dts_backend::{
    ProductResource, ProductResourceFilter, ProductResourceRepo, ResourceWrite,
    repo::{
        assets::{validation, *},
        metadata_lineage::MetadataLineageRepo,
    },
};
use serde_json::{Value, json};

use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        product::{ResourceAction, ResourceInput, ResourceUpdate},
        product_domain::warehouse,
    },
};

pub mod graph;

#[derive(Clone)]
pub struct AssetsService {
    repo: Arc<dyn AssetsRepo>,
    resources: Arc<dyn ProductResourceRepo>,
    lineage: Arc<dyn MetadataLineageRepo>,
}

impl AssetsService {
    pub fn new(
        repo: Arc<dyn AssetsRepo>,
        resources: Arc<dyn ProductResourceRepo>,
        lineage: Arc<dyn MetadataLineageRepo>,
    ) -> Self {
        Self {
            repo,
            resources,
            lineage,
        }
    }
    pub fn handles(kind: &str) -> bool {
        matches!(kind, "asset_entry" | "asset_map" | "asset_analysis")
    }

    pub async fn create(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        input: ResourceInput,
    ) -> Result<ProductResource, AppError> {
        Ok(self
            .repo
            .save(&params(
                actor,
                kind,
                uuid::Uuid::new_v4().to_string(),
                None,
                input,
            )?)
            .await?)
    }

    pub async fn update(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        input: ResourceUpdate,
    ) -> Result<ProductResource, AppError> {
        let version = input.expected_version;
        let body = ResourceInput {
            code: input.code,
            name: input.name,
            description: input.description,
            parent_id: input.parent_id,
            properties: input.properties,
        };
        Ok(self
            .repo
            .save(&params(actor, kind, id.into(), Some(version), body)?)
            .await?)
    }

    pub async fn delete(
        &self,
        actor: &AuthIdentity,
        kind: &str,
        id: &str,
        version: i64,
    ) -> Result<(), AppError> {
        Ok(self
            .repo
            .delete_draft(kind, &scope(actor, id, version))
            .await?)
    }

    pub async fn get(&self, actor: &AuthIdentity, id: &str) -> Result<AssetView, AppError> {
        self.repo
            .get_asset(&AssetReadParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                id: id.into(),
            })
            .await?
            .ok_or_else(|| AppError::NotFound("资产不存在".into()))
    }

    pub async fn list(
        &self,
        actor: &AuthIdentity,
        filter: ProductResourceFilter,
    ) -> Result<AssetPage, AppError> {
        if filter.tenant_id != actor.tenant_id {
            return Err(AppError::Forbidden("资产查询租户不一致".into()));
        }
        Ok(self
            .repo
            .list_assets(&AssetListParams {
                actor_id: actor.sub.clone(),
                filter,
            })
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
        if kind == "asset_map" && action == "analyze" {
            return self.analyze_map(actor, id, input).await;
        }
        if kind != "asset_entry" {
            return Err(AppError::BadRequest("资产模块不支持此操作".into()));
        }
        if !input.payload.is_null() && input.payload.as_object().is_none_or(|p| !p.is_empty()) {
            return Err(AppError::BadRequest(
                "资产状态操作不接受来源证明或自定义结果".into(),
            ));
        }
        let action = match action {
            "register" => AssetTransition::Register,
            "publish" => AssetTransition::Publish,
            "withdraw" => AssetTransition::Withdraw,
            _ => return Err(AppError::BadRequest("未知资产操作".into())),
        };
        let validated_source = if action == AssetTransition::Withdraw {
            None
        } else {
            Some(
                self.validate_source(actor, id, input.expected_version)
                    .await?,
            )
        };
        let view = self
            .repo
            .transition(&TransitionAssetParams {
                scope: scope(actor, id, input.expected_version),
                action,
                validated_source,
            })
            .await?;
        Ok(
            json!({"resource":view.resource,"result":{"publication":view.publication,"source_valid":view.source_valid,"stale":view.stale,
            "invalid_reason":view.invalid_reason,"current_source_version":view.current_source_version,"publication_scope":"catalog_definition","physical_published":false}}),
        )
    }

    async fn validate_source(
        &self,
        actor: &AuthIdentity,
        id: &str,
        version: i64,
    ) -> Result<AssetSourceCheck, AppError> {
        let asset = self.get(actor, id).await?.resource;
        if asset.version != version {
            return Err(AppError::Conflict("资产已经变更".into()));
        }
        let kind = validation::text(&asset.properties, "object_kind")?;
        let id = validation::text(&asset.properties, "object_id")?;
        let source = self
            .resources
            .get(&actor.tenant_id, kind, id)
            .await?
            .ok_or_else(|| AppError::NotFound("资产来源不存在".into()))?;
        warehouse::validate(kind, &source.properties)?;
        Ok(AssetSourceCheck {
            kind: kind.into(),
            id: id.into(),
            version: source.version,
            definition_hash: validation::hash(&source.properties)?,
        })
    }

    pub async fn analyze_usage(
        &self,
        actor: &AuthIdentity,
        request: AssetUsageRequest,
    ) -> Result<AssetUsage, AppError> {
        Ok(self
            .repo
            .analyze_usage(&AssetUsageParams {
                tenant_id: actor.tenant_id.clone(),
                actor_id: actor.sub.clone(),
                request,
            })
            .await?)
    }

    async fn analyze_map(
        &self,
        actor: &AuthIdentity,
        id: &str,
        input: ResourceAction,
    ) -> Result<Value, AppError> {
        let request: graph::AssetMapRequest = if input.payload.is_null() {
            graph::AssetMapRequest::default()
        } else {
            serde_json::from_value(input.payload)
                .map_err(|_| AppError::BadRequest("资产地图分析参数无效".into()))?
        };
        request.validate()?;
        let resource = self
            .resources
            .get(&actor.tenant_id, "asset_map", id)
            .await?
            .ok_or_else(|| AppError::NotFound("资产地图不存在".into()))?;
        let snapshot = self
            .repo
            .map_snapshot(&AssetMapSnapshotParams {
                scope: scope(actor, id, input.expected_version),
                max_nodes: request.max_nodes,
                max_edges: request.max_edges,
            })
            .await?;
        if resource.version != input.expected_version {
            return Err(AppError::Conflict("资产地图已变更".into()));
        }
        let lineage = if snapshot.can_read_metadata
            && snapshot
                .nodes
                .iter()
                .any(|n| n.source_kind == "metadata_object")
        {
            Some(
                self.lineage
                    .snapshot(
                        &dts_backend::repo::metadata_lineage::LineageSnapshotParams {
                            tenant_id: actor.tenant_id.clone(),
                            actor_id: actor.sub.clone(),
                            max_edges: 2000,
                            root_object_id: None,
                        },
                    )
                    .await?,
            )
        } else {
            None
        };
        Ok(json!({"resource":resource,"result":graph::project(snapshot,lineage,request.max_edges)}))
    }
}

fn scope(actor: &AuthIdentity, id: &str, version: i64) -> AssetScope {
    AssetScope {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
        id: id.into(),
        expected_version: version,
    }
}

fn params(
    actor: &AuthIdentity,
    kind: &str,
    id: String,
    version: Option<i64>,
    input: ResourceInput,
) -> Result<SaveAssetParams, AppError> {
    let references = validation::references(kind, &input.properties)?;
    let params = SaveAssetParams {
        tenant_id: actor.tenant_id.clone(),
        actor_id: actor.sub.clone(),
        id,
        kind: kind.into(),
        expected_version: version,
        fields: ResourceWrite {
            code: input.code.trim().into(),
            name: input.name.trim().into(),
            description: input.description,
            parent_id: input.parent_id,
            status: "draft".into(),
            properties: input.properties,
            references,
        },
    };
    validation::fields(&params)?;
    Ok(params)
}
