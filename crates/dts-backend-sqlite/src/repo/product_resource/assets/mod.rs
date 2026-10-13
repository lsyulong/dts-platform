use async_trait::async_trait;
use dts_backend::{
    BackendError, ProductResource,
    repo::assets::{validation::*, *},
};

use super::{SqliteProductResourceRepo as Resources, helpers};

mod authority;
mod maps;
mod proof;
mod reads;
mod storage;
mod usage;

#[async_trait]
impl AssetsRepo for Resources {
    async fn save(&self, params: &SaveAssetParams) -> Result<ProductResource, BackendError> {
        fields(params)?;
        let mut tx = self.begin_write(&params.tenant_id).await?;
        let permissions =
            authority::permissions(&mut tx, &params.tenant_id, &params.actor_id, true).await?;
        authority::require(&permissions, "assets:write")?;
        let before =
            Self::read_resource(&mut tx, &params.tenant_id, &params.kind, &params.id).await?;
        match (&before, params.expected_version) {
            (None, None) => {},
            (Some(r), Some(v))
                if r.version == v && matches!(r.status.as_str(), "draft" | "withdrawn") => {},
            _ => {
                return Err(BackendError::Conflict(
                    "资产必须是当前草稿或已撤回版本".into(),
                ));
            },
        }
        if params.kind == "asset_entry" {
            let kind = text(&params.fields.properties, "object_kind")?;
            authority::require(
                &permissions,
                source_permission(kind).ok_or_else(|| invalid("资产来源无效"))?,
            )?;
            // Real existence/tenant/active owner/domain constraints are enforced below in the same write transaction.
        } else {
            maps::validate_saved(&mut tx, params, &permissions).await?;
        }
        let resource = storage::write(
            &mut tx,
            params,
            before.as_ref(),
            if before.is_some() { "update" } else { "create" },
        )
        .await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(resource)
    }

    async fn transition(&self, params: &TransitionAssetParams) -> Result<AssetView, BackendError> {
        let scope = &params.scope;
        let mut tx = self.begin_write(&scope.tenant_id).await?;
        let permissions =
            authority::permissions(&mut tx, &scope.tenant_id, &scope.actor_id, true).await?;
        authority::require(
            &permissions,
            if params.action == AssetTransition::Register {
                "assets:write"
            } else {
                "assets:execute"
            },
        )?;
        let before = storage::current(&mut tx, "asset_entry", scope).await?;
        let kind = text(&before.properties, "object_kind")?;
        authority::require(
            &permissions,
            source_permission(kind).ok_or_else(|| invalid("资产来源无效"))?,
        )?;
        let (status, operation) = match (params.action, before.status.as_str()) {
            (AssetTransition::Register, "draft" | "withdrawn") => ("registered", "register"),
            (AssetTransition::Publish, "registered") => ("published", "publish"),
            (AssetTransition::Withdraw, "registered" | "published") => ("withdrawn", "withdraw"),
            _ => return Err(BackendError::Conflict("当前资产状态不支持此操作".into())),
        };
        let source = if params.action != AssetTransition::Withdraw {
            Some(
                proof::source(&mut tx, &before)
                    .await?
                    .ok_or_else(|| invalid("资产没有当前真实来源证明"))?,
            )
        } else {
            None
        };
        if let Some(source) = &source {
            let definition_hash = hash(&source.resource.properties)?;
            if params.validated_source.as_ref().is_none_or(|check| {
                check.kind != source.resource.kind
                    || check.id != source.resource.id
                    || check.version != source.resource.version
                    || definition_hash != check.definition_hash
            }) {
                return Err(BackendError::Conflict(
                    "资产来源在定义验证后已变化，请刷新重试".into(),
                ));
            }
        }
        let saved = storage::write(
            &mut tx,
            &storage::transition_params(scope, &before, status),
            Some(&before),
            operation,
        )
        .await?;
        if params.action == AssetTransition::Publish {
            let source = source.as_ref().ok_or_else(|| invalid("发布缺少来源证明"))?;
            proof::publish(&mut tx, &saved, source, &scope.actor_id).await?;
        }
        let view = proof::view(&mut tx, saved).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(view)
    }

    async fn delete_draft(&self, kind: &str, scope: &AssetScope) -> Result<(), BackendError> {
        if !matches!(kind, "asset_entry" | "asset_map") {
            return Err(invalid("不是可编辑资产模块"));
        }
        let mut tx = self.begin_write(&scope.tenant_id).await?;
        let permissions =
            authority::permissions(&mut tx, &scope.tenant_id, &scope.actor_id, true).await?;
        authority::require(&permissions, "assets:write")?;
        let before = storage::current(&mut tx, kind, scope).await?;
        if !matches!(before.status.as_str(), "draft" | "withdrawn") {
            return Err(BackendError::Conflict("先撤回资产再删除草稿".into()));
        }
        if kind == "asset_entry" {
            authority::require(
                &permissions,
                source_permission(text(&before.properties, "object_kind")?)
                    .ok_or_else(|| invalid("来源无效"))?,
            )?;
        }
        Self::write_references(&mut tx, &scope.tenant_id, kind, &scope.id, &[]).await?;
        sqlx::query(
            "DELETE FROM product_resource WHERE tenant_id=?1 AND kind=?2 AND id=?3 AND version=?4",
        )
        .bind(&scope.tenant_id)
        .bind(kind)
        .bind(&scope.id)
        .bind(scope.expected_version)
        .execute(&mut *tx)
        .await
        .map_err(helpers::map_error)?;
        helpers::audit(&mut tx, &scope.actor_id, "delete", Some(&before), None).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(())
    }

    async fn get_asset(&self, params: &AssetReadParams) -> Result<Option<AssetView>, BackendError> {
        let mut tx = self.pool.begin().await.map_err(helpers::map_error)?;
        let permissions =
            authority::permissions(&mut tx, &params.tenant_id, &params.actor_id, false).await?;
        authority::require(&permissions, "assets:read")?;
        let Some(asset) =
            Self::read_resource(&mut tx, &params.tenant_id, "asset_entry", &params.id).await?
        else {
            return Ok(None);
        };
        authority::require(
            &permissions,
            source_permission(text(&asset.properties, "object_kind")?)
                .ok_or_else(|| invalid("资产来源类型无效"))?,
        )?;
        let view = proof::view(&mut tx, asset).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(Some(view))
    }

    async fn list_assets(&self, params: &AssetListParams) -> Result<AssetPage, BackendError> {
        let mut tx = self.pool.begin().await.map_err(helpers::map_error)?;
        let permissions =
            authority::permissions(&mut tx, &params.filter.tenant_id, &params.actor_id, false)
                .await?;
        authority::require(&permissions, "assets:read")?;
        let result = reads::list(&mut tx, params, &permissions).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(result)
    }

    async fn map_snapshot(
        &self,
        params: &AssetMapSnapshotParams,
    ) -> Result<AssetMapSnapshot, BackendError> {
        let mut tx = self.pool.begin().await.map_err(helpers::map_error)?;
        let permissions = authority::permissions(
            &mut tx,
            &params.scope.tenant_id,
            &params.scope.actor_id,
            false,
        )
        .await?;
        authority::require(&permissions, "assets:read")?;
        let snapshot = maps::snapshot(&mut tx, params, &permissions).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(snapshot)
    }

    async fn analyze_usage(&self, params: &AssetUsageParams) -> Result<AssetUsage, BackendError> {
        let mut tx = self.pool.begin().await.map_err(helpers::map_error)?;
        let permissions =
            authority::permissions(&mut tx, &params.tenant_id, &params.actor_id, false).await?;
        authority::require(&permissions, "assets:read")?;
        let result = usage::analyze(&mut tx, params, &permissions).await?;
        tx.commit().await.map_err(helpers::map_error)?;
        Ok(result)
    }
}
