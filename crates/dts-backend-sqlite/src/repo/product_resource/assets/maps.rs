use std::collections::BTreeSet;

use dts_backend::{
    BackendError,
    repo::assets::{validation::*, *},
};
use serde_json::json;
use sqlx::SqliteConnection;

use super::{authority, proof, storage};

pub(super) async fn snapshot(
    conn: &mut SqliteConnection,
    params: &AssetMapSnapshotParams,
    permissions: &BTreeSet<String>,
) -> Result<AssetMapSnapshot, BackendError> {
    if !(1..=500).contains(&params.max_nodes) || !(1..=2000).contains(&params.max_edges) {
        return Err(invalid("资产地图节点或边界限无效"));
    }
    let map = storage::current(conn, "asset_map", &params.scope).await?;
    let ids = asset_ids(&map.properties)?;
    let domain = map.properties["domain_id"]
        .as_str()
        .filter(|v| !v.is_empty());
    let mut views = Vec::new();
    let mut nodes = Vec::new();
    let mut invalid_nodes = Vec::new();
    let mut reasons = Vec::new();
    for id in ids {
        let asset = storage::get(conn, &params.scope.tenant_id, "asset_entry", &id).await?;
        let kind = asset.properties["object_kind"].as_str().unwrap_or_default();
        // Omit inaccessible sources without returning their object identifiers.
        if source_permission(kind).is_none_or(|p| !permissions.contains(p)) {
            continue;
        }
        if domain.is_some_and(|domain| asset.properties["domain_id"].as_str() != Some(domain)) {
            invalid_nodes.push(InvalidAssetNode {
                id,
                reason: "asset_domain_changed".into(),
            });
            continue;
        }
        if nodes.len() >= params.max_nodes as usize {
            reasons.push("node_limit".into());
            break;
        }
        let view = proof::view(conn, asset).await?;
        let Some(version) = view.current_source_version else {
            invalid_nodes.push(InvalidAssetNode {
                id,
                reason: view
                    .invalid_reason
                    .unwrap_or_else(|| "source_missing".into()),
            });
            continue;
        };
        let asset = &view.resource;
        nodes.push(AssetGraphNode {
            id: asset.id.clone(),
            code: asset.code.clone(),
            name: asset.name.clone(),
            status: asset.status.clone(),
            version: asset.version,
            source_kind: text(&asset.properties, "object_kind")?.into(),
            source_id: text(&asset.properties, "object_id")?.into(),
            source_version: version,
            publication_version: view.publication.as_ref().map(|p| p.asset_version),
            stale: view.stale,
        });
        views.push(view);
    }
    let mut edges = Vec::new();
    'sources: for view in &views {
        if !view.source_valid {
            continue;
        }
        let Some(source) = proof::source(conn, &view.resource).await? else {
            continue;
        };
        for reference in source.resource.references {
            for target in nodes.iter().filter(|node| {
                node.source_kind == reference.target_kind
                    && node.source_id == reference.target_id
                    && !node.stale
            }) {
                if edges.len() >= params.max_edges as usize {
                    reasons.push("edge_limit".into());
                    break 'sources;
                }
                edges.push(AssetGraphEdge { source_asset_id:view.resource.id.clone(), target_asset_id:target.id.clone(), origin:"resource_reference".into(),
                    evidence:json!({"source_kind":source.resource.kind,"source_id":source.resource.id,"source_version":source.resource.version,
                        "field":reference.field,"target_kind":reference.target_kind,"target_id":reference.target_id,"target_version":target.source_version}) });
            }
        }
    }
    reasons.sort();
    reasons.dedup();
    Ok(AssetMapSnapshot {
        nodes,
        edges,
        invalid_nodes,
        truncated: !reasons.is_empty(),
        truncation_reasons: reasons,
        can_read_metadata: permissions.contains("metadata:read"),
    })
}

pub(super) async fn validate_saved(
    conn: &mut SqliteConnection,
    params: &SaveAssetParams,
    permissions: &BTreeSet<String>,
) -> Result<(), BackendError> {
    let domain = params.fields.properties["domain_id"]
        .as_str()
        .filter(|v| !v.is_empty());
    for id in asset_ids(&params.fields.properties)? {
        let asset = storage::get(conn, &params.tenant_id, "asset_entry", &id).await?;
        let kind = text(&asset.properties, "object_kind")?;
        authority::require(
            permissions,
            source_permission(kind).ok_or_else(|| invalid("资产来源类型无效"))?,
        )?;
        if domain.is_some_and(|domain| asset.properties["domain_id"].as_str() != Some(domain)) {
            return Err(invalid("资产节点不属于地图数据域"));
        }
    }
    Ok(())
}
