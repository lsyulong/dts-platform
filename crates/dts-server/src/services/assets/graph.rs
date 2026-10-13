use dts_backend::repo::{assets::*, metadata_lineage::LineageSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::models::error::AppError;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssetMapRequest {
    pub max_nodes: u32,
    pub max_edges: u32,
}
impl Default for AssetMapRequest {
    fn default() -> Self {
        Self {
            max_nodes: 200,
            max_edges: 500,
        }
    }
}
impl AssetMapRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(1..=500).contains(&self.max_nodes) || !(1..=2000).contains(&self.max_edges) {
            return Err(AppError::BadRequest(
                "资产地图支持最多500节点和2000条边".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct AssetMapAnalysis {
    pub nodes: Vec<AssetGraphNode>,
    pub edges: Vec<AssetGraphEdge>,
    pub stale_edges: Vec<AssetGraphEdge>,
    pub invalid_nodes: Vec<InvalidAssetNode>,
    pub truncated: bool,
    pub truncation_reasons: Vec<String>,
    pub scanned_lineage_edges: usize,
    pub invalid_lineage_edges: usize,
    pub lineage_included: bool,
    pub relation_scope: String,
}

pub fn project(
    mut snapshot: AssetMapSnapshot,
    lineage: Option<LineageSnapshot>,
    max_edges: u32,
) -> AssetMapAnalysis {
    let mut stale_edges = Vec::new();
    let mut scanned_lineage_edges = 0;
    let mut invalid_lineage_edges = 0;
    let lineage_included = lineage.is_some();
    if let Some(lineage) = lineage {
        scanned_lineage_edges = lineage.scanned_edges;
        invalid_lineage_edges = lineage.invalid_edges.len();
        if lineage.scan_truncated {
            snapshot
                .truncation_reasons
                .push("lineage_scan_limit".into());
        }
        let versions: std::collections::HashMap<_, _> = lineage
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.version))
            .collect();
        'lineage: for edge in &lineage.edges {
            let sources: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|n| {
                    n.source_kind == "metadata_object" && n.source_id == edge.source_object_id
                })
                .collect();
            let targets: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|n| {
                    n.source_kind == "metadata_object" && n.source_id == edge.target_object_id
                })
                .collect();
            for source in &sources {
                for target in &targets {
                    if snapshot.edges.len() + stale_edges.len() >= max_edges as usize {
                        snapshot.truncation_reasons.push("edge_limit".into());
                        break 'lineage;
                    }
                    let projected = AssetGraphEdge {
                        source_asset_id: source.id.clone(),
                        target_asset_id: target.id.clone(),
                        origin: "metadata_lineage".into(),
                        evidence: json!({"lineage_id":edge.id,"lineage_version":edge.version,"origin":edge.origin,"evidence":edge.evidence,
                            "evidence_actor_id":edge.evidence_actor_id,"evidence_recorded_at":edge.evidence_recorded_at,
                            "source_object_id":edge.source_object_id,"target_object_id":edge.target_object_id,
                            "source_object_version":edge.source_object_version,"target_object_version":edge.target_object_version}),
                    };
                    let current = !edge.stale
                        && !source.stale
                        && !target.stale
                        && versions.get(source.source_id.as_str()).copied()
                            == Some(source.source_version)
                        && versions.get(target.source_id.as_str()).copied()
                            == Some(target.source_version);
                    if current {
                        snapshot.edges.push(projected);
                    } else {
                        stale_edges.push(projected);
                    }
                }
            }
        }
    }
    snapshot.truncation_reasons.sort();
    snapshot.truncation_reasons.dedup();
    AssetMapAnalysis {
        nodes: snapshot.nodes,
        edges: snapshot.edges,
        stale_edges,
        invalid_nodes: snapshot.invalid_nodes,
        truncated: snapshot.truncated || !snapshot.truncation_reasons.is_empty(),
        truncation_reasons: snapshot.truncation_reasons,
        scanned_lineage_edges,
        invalid_lineage_edges,
        lineage_included,
        relation_scope: "saved_resource_references_and_current_manual_metadata_lineage".into(),
    }
}

#[cfg(test)]
mod tests;
