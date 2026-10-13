use crate::models::error::AppError;
use dts_backend::repo::metadata_lineage::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LineageAnalysisRequest {
    pub root_object_id: Option<String>,
    pub direction: String,
    pub max_depth: u32,
    pub max_nodes: u32,
    pub max_edges: u32,
}
impl Default for LineageAnalysisRequest {
    fn default() -> Self {
        Self {
            root_object_id: None,
            direction: "downstream".into(),
            max_depth: 5,
            max_nodes: 100,
            max_edges: 200,
        }
    }
}
impl LineageAnalysisRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if !["upstream", "downstream", "both"].contains(&self.direction.as_str())
            || !(1..=20).contains(&self.max_depth)
            || !(1..=500).contains(&self.max_nodes)
            || !(1..=1000).contains(&self.max_edges)
            || self.root_object_id.as_ref().is_some_and(|id| {
                id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control)
            })
        {
            return Err(AppError::BadRequest("血缘方向或遍历范围无效".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct LineagePath {
    pub node_id: String,
    pub direction: String,
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub depth: u32,
}
#[derive(Debug, Serialize)]
pub struct LineageAnalysis {
    pub root_object_id: String,
    pub direction: String,
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
    pub paths: Vec<LineagePath>,
    pub visited: usize,
    pub truncated: bool,
    pub truncation_reasons: Vec<String>,
    pub stale_edges: Vec<LineageEdge>,
    pub invalid_edges: Vec<InvalidLineageEdge>,
    pub cycles_detected: bool,
    pub scanned_edges: usize,
    pub total_edges: i64,
}

pub(super) fn analyze(
    snapshot: LineageSnapshot,
    request: &LineageAnalysisRequest,
) -> Result<LineageAnalysis, AppError> {
    request.validate()?;
    let root = request
        .root_object_id
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("缺少分析起点".into()))?;
    let nodes: HashMap<String, LineageNode> = snapshot
        .nodes
        .into_iter()
        .map(|node| (node.id.clone(), node))
        .collect();
    if !nodes.contains_key(root) {
        return Err(AppError::NotFound(
            "起点不是当前租户中有真实采集证明的元数据对象".into(),
        ));
    }
    let mut selected = HashSet::from([root.clone()]);
    let mut edge_ids = HashSet::new();
    let mut paths = Vec::new();
    let mut reasons = BTreeSet::new();
    let mut cycles = false;
    if snapshot.scan_truncated {
        reasons.insert("scan_limit".into());
    }
    let directions = if request.direction == "both" {
        vec!["upstream", "downstream"]
    } else {
        vec![request.direction.as_str()]
    };
    for direction in directions {
        let mut seen = HashSet::from([root.clone()]);
        let mut queue = VecDeque::from([(root.clone(), vec![root.clone()], Vec::<String>::new())]);
        while let Some((current, node_path, edge_path)) = queue.pop_front() {
            let related: Vec<&LineageEdge> = snapshot
                .edges
                .iter()
                .filter(|edge| {
                    !edge.stale
                        && edge.invalid_reason.is_none()
                        && if direction == "upstream" {
                            edge.target_object_id == current
                        } else {
                            edge.source_object_id == current
                        }
                })
                .collect();
            if edge_path.len() >= request.max_depth as usize {
                if !related.is_empty() {
                    reasons.insert("depth_limit".into());
                }
                continue;
            }
            for edge in related {
                let next = if direction == "upstream" {
                    &edge.source_object_id
                } else {
                    &edge.target_object_id
                };
                if !nodes.contains_key(next) {
                    continue;
                }
                if !selected.contains(next) && selected.len() >= request.max_nodes as usize {
                    reasons.insert("node_limit".into());
                    continue;
                }
                if !edge_ids.contains(&edge.id) && edge_ids.len() >= request.max_edges as usize {
                    reasons.insert("edge_limit".into());
                    continue;
                }
                selected.insert(next.clone());
                edge_ids.insert(edge.id.clone());
                if node_path.contains(next) {
                    cycles = true;
                    continue;
                }
                if seen.insert(next.clone()) {
                    let mut next_nodes = node_path.clone();
                    next_nodes.push(next.clone());
                    let mut next_edges = edge_path.clone();
                    next_edges.push(edge.id.clone());
                    paths.push(LineagePath {
                        node_id: next.clone(),
                        direction: direction.into(),
                        node_ids: next_nodes.clone(),
                        edge_ids: next_edges.clone(),
                        depth: next_edges.len() as u32,
                    });
                    queue.push_back((next.clone(), next_nodes, next_edges));
                }
            }
        }
    }
    let visited = selected.len();
    let mut nodes: Vec<_> = nodes
        .into_values()
        .filter(|node| selected.contains(&node.id))
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let edges: Vec<LineageEdge> = snapshot
        .edges
        .iter()
        .filter(|edge| edge_ids.contains(&edge.id))
        .cloned()
        .collect();
    cycles |= has_cycle(&selected, &edges);
    let stale_edges = snapshot
        .edges
        .into_iter()
        .filter(|edge| edge.stale)
        .collect();
    Ok(LineageAnalysis {
        root_object_id: root.clone(),
        direction: request.direction.clone(),
        nodes,
        edges,
        paths,
        visited,
        truncated: !reasons.is_empty(),
        truncation_reasons: reasons.into_iter().collect(),
        stale_edges,
        invalid_edges: snapshot.invalid_edges,
        cycles_detected: cycles,
        scanned_edges: snapshot.scanned_edges,
        total_edges: snapshot.total_edges,
    })
}

fn has_cycle(nodes: &HashSet<String>, edges: &[LineageEdge]) -> bool {
    let mut degree: HashMap<String, usize> = nodes.iter().cloned().map(|id| (id, 0)).collect();
    let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        if let Some(count) = degree.get_mut(&edge.target_object_id) {
            *count += 1;
        }
        next.entry(&edge.source_object_id)
            .or_default()
            .push(&edge.target_object_id);
    }
    let mut queue: VecDeque<String> = degree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut count = 0;
    while let Some(id) = queue.pop_front() {
        count += 1;
        for target in next.get(id.as_str()).into_iter().flatten() {
            if let Some(degree) = degree.get_mut(*target) {
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back((*target).into());
                }
            }
        }
    }
    count < nodes.len()
}

#[cfg(test)]
mod tests;
