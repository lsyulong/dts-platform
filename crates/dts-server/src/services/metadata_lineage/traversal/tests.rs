use super::*;
use chrono::Utc;

fn node(id: &str) -> LineageNode {
    LineageNode {
        id: id.into(),
        code: id.into(),
        name: id.into(),
        version: 1,
        status: "published".into(),
        table_catalog: "warehouse".into(),
        schema_name: "main".into(),
        table_name: id.into(),
        source_fingerprint: "a".repeat(64),
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
        evidence: "Confirmed upstream definition".into(),
        evidence_actor_id: "actor".into(),
        evidence_recorded_at: Utc::now(),
        stale: false,
        invalid_reason: None,
    }
}
fn graph() -> LineageSnapshot {
    LineageSnapshot {
        nodes: vec![node("a"), node("b"), node("c")],
        edges: vec![edge("ab", "a", "b"), edge("bc", "b", "c")],
        invalid_edges: vec![],
        total_edges: 2,
        scanned_edges: 2,
        scan_truncated: false,
    }
}
fn request(root: &str, direction: &str) -> LineageAnalysisRequest {
    LineageAnalysisRequest {
        root_object_id: Some(root.into()),
        direction: direction.into(),
        ..Default::default()
    }
}

#[test]
fn actual_upstream_and_downstream_paths_are_computed_without_inventing_nodes() {
    let result = analyze(graph(), &request("b", "both")).expect("analysis");
    assert_eq!(result.visited, 3);
    assert_eq!(result.edges.len(), 2);
    assert!(!result.truncated);
    let upstream = result
        .paths
        .iter()
        .find(|path| path.direction == "upstream")
        .expect("upstream");
    assert_eq!(upstream.node_ids, vec!["b", "a"]);
    assert_eq!(upstream.edge_ids, vec!["ab"]);
    let downstream = result
        .paths
        .iter()
        .find(|path| path.direction == "downstream")
        .expect("downstream");
    assert_eq!(downstream.node_id, "c");
    assert_eq!(downstream.depth, 1);
    assert!(analyze(graph(), &request("missing", "downstream")).is_err());
}
#[test]
fn depth_node_edge_and_snapshot_limits_are_explicit() {
    let result = analyze(
        graph(),
        &LineageAnalysisRequest {
            max_depth: 1,
            ..request("a", "downstream")
        },
    )
    .expect("depth");
    assert!(result.truncated);
    assert!(result.truncation_reasons.contains(&"depth_limit".into()));
    assert_eq!(result.visited, 2);
    let result = analyze(
        graph(),
        &LineageAnalysisRequest {
            max_nodes: 1,
            ..request("a", "downstream")
        },
    )
    .expect("nodes");
    assert_eq!(result.visited, 1);
    assert!(result.truncation_reasons.contains(&"node_limit".into()));
    let result = analyze(
        graph(),
        &LineageAnalysisRequest {
            max_edges: 1,
            ..request("a", "downstream")
        },
    )
    .expect("edges");
    assert_eq!(result.edges.len(), 1);
    assert!(result.truncation_reasons.contains(&"edge_limit".into()));
    let mut partial = graph();
    partial.scan_truncated = true;
    partial.total_edges = 2001;
    assert!(
        analyze(partial, &request("a", "downstream"))
            .expect("snapshot")
            .truncation_reasons
            .contains(&"scan_limit".into())
    );
}
#[test]
fn stale_evidence_never_establishes_current_impact() {
    let mut stale = graph();
    stale.edges[0].stale = true;
    stale.edges[0].invalid_reason = Some("endpoint_version_changed".into());
    let result = analyze(stale, &request("a", "downstream")).expect("stale");
    assert_eq!(result.visited, 1);
    assert!(result.edges.is_empty());
    assert_eq!(result.stale_edges.len(), 1);
}
#[test]
fn corrupt_imported_cycles_are_reported_with_bounded_real_edges() {
    let mut cycle = graph();
    cycle.edges.push(edge("ca", "c", "a"));
    cycle.total_edges = 3;
    cycle.scanned_edges = 3;
    let result = analyze(cycle, &request("a", "downstream")).expect("bounded cycle");
    assert!(result.cycles_detected);
    assert_eq!(result.visited, 3);
    assert_eq!(result.edges.len(), 3);
}
