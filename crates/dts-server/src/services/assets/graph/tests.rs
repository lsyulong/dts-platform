use super::*;
use chrono::Utc;
use dts_backend::repo::metadata_lineage::{LineageEdge, LineageNode};

fn node(asset: &str, object: &str, version: i64) -> AssetGraphNode {
    AssetGraphNode {
        id: asset.into(),
        code: asset.into(),
        name: asset.into(),
        status: "registered".into(),
        version: 2,
        source_kind: "metadata_object".into(),
        source_id: object.into(),
        source_version: version,
        publication_version: None,
        stale: false,
    }
}
fn snapshot() -> AssetMapSnapshot {
    AssetMapSnapshot {
        nodes: vec![node("a", "m1", 1), node("b", "m2", 1)],
        edges: vec![],
        invalid_nodes: vec![],
        truncated: false,
        truncation_reasons: vec![],
        can_read_metadata: true,
    }
}
fn lineage_node(id: &str, version: i64) -> LineageNode {
    LineageNode {
        id: id.into(),
        code: id.into(),
        name: id.into(),
        version,
        status: "published".into(),
        table_catalog: "warehouse".into(),
        schema_name: "main".into(),
        table_name: id.into(),
        source_fingerprint: "actual".into(),
    }
}
fn edge(id: &str, source: &str, target: &str) -> LineageEdge {
    LineageEdge {
        id: id.into(),
        version: 1,
        source_object_id: source.into(),
        target_object_id: target.into(),
        source_object_version: 1,
        target_object_version: 1,
        origin: "manual".into(),
        evidence: "Recorded business transformation".into(),
        evidence_actor_id: "owner".into(),
        evidence_recorded_at: Utc::now(),
        stale: false,
        invalid_reason: None,
    }
}
fn lineage() -> LineageSnapshot {
    LineageSnapshot {
        nodes: vec![
            lineage_node("m1", 1),
            lineage_node("m2", 1),
            lineage_node("m3", 1),
        ],
        edges: vec![edge("e1", "m1", "m2"), edge("e2", "m2", "m3")],
        invalid_edges: vec![],
        total_edges: 2,
        scanned_edges: 2,
        scan_truncated: false,
    }
}

#[test]
fn projects_only_real_selected_assets_with_provenance() {
    let result = project(snapshot(), Some(lineage()), 10);
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].source_asset_id, "a");
    assert_eq!(result.edges[0].target_asset_id, "b");
    assert_eq!(result.edges[0].evidence["lineage_id"], "e1");
    assert!(!result.nodes.iter().any(|n| n.source_id == "m3"));
}

#[test]
fn version_changed_or_stale_lineage_never_becomes_current_edge() {
    let mut later = lineage();
    later.nodes[1].version = 2;
    let result = project(snapshot(), Some(later), 10);
    assert!(result.edges.is_empty());
    assert_eq!(result.stale_edges.len(), 1);
    let mut later = lineage();
    later.edges[0].stale = true;
    assert_eq!(project(snapshot(), Some(later), 10).stale_edges.len(), 1);
}

#[test]
fn no_lineage_authorization_yields_only_actual_reference_edges() {
    let result = project(snapshot(), None, 10);
    assert!(result.edges.is_empty());
    assert!(!result.lineage_included);
    assert_eq!(result.scanned_lineage_edges, 0);
}

#[test]
fn both_resource_and_projection_edges_share_the_same_cap() {
    let mut stored = snapshot();
    stored.edges.push(AssetGraphEdge {
        source_asset_id: "a".into(),
        target_asset_id: "b".into(),
        origin: "resource_reference".into(),
        evidence: json!({"field":"real"}),
    });
    let result = project(stored, Some(lineage()), 1);
    assert_eq!(result.edges.len(), 1);
    assert!(result.stale_edges.is_empty());
    assert!(result.truncated);
    assert!(result.truncation_reasons.contains(&"edge_limit".to_owned()));
}
