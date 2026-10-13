// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::*;

pub struct EncodedJobNode {
    pub id: String,
    pub name: String,
    pub definition: String,
    pub depends_on: String,
}

pub struct EncodedProductJob {
    pub definition_snapshot: String,
    pub parameters: String,
    pub nodes: Vec<EncodedJobNode>,
}

fn invalid(message: &str) -> BackendError {
    BackendError::ConstraintViolation(message.into())
}

pub fn bounded_json(value: &Value, maximum: usize) -> Result<String, BackendError> {
    let encoded = serde_json::to_string(value)
        .map_err(|error| BackendError::Internal(format!("运行 JSON 序列化失败: {error}")))?;
    if encoded.len() > maximum {
        return Err(invalid("运行快照或结果超过允许大小"));
    }
    Ok(encoded)
}

pub fn bounded_text(value: &str, maximum: usize) -> Result<(), BackendError> {
    if value.trim().is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(invalid("运行标识或错误信息为空、过长或包含无效字符"));
    }
    Ok(())
}

pub fn lease_seconds(seconds: u32) -> Result<i64, BackendError> {
    if !(5..=300).contains(&seconds) {
        return Err(invalid("租约长度必须为 5 至 300 秒"));
    }
    Ok(i64::from(seconds))
}

pub fn enqueue(params: &EnqueueProductJobParams) -> Result<EncodedProductJob, BackendError> {
    for text in [
        &params.id,
        &params.tenant_id,
        &params.source_kind,
        &params.source_id,
        &params.actor_id,
        &params.action,
        &params.idempotency_key,
    ] {
        bounded_text(text, 128)?;
    }
    if params.source_version < 1 || params.nodes.is_empty() || params.nodes.len() > 256 {
        return Err(invalid("来源版本无效或节点数量必须为 1 至 256"));
    }
    if !params.definition_snapshot.is_object() || !params.parameters.is_object() {
        return Err(invalid("运行定义和参数必须为 JSON 对象"));
    }
    let definition_snapshot = bounded_json(&params.definition_snapshot, 512 * 1024)?;
    let parameters = bounded_json(&params.parameters, 64 * 1024)?;
    validate_dag(&params.nodes)?;
    let nodes = params
        .nodes
        .iter()
        .map(|node| {
            bounded_text(&node.id, 128)?;
            bounded_text(&node.name, 256)?;
            Ok(EncodedJobNode {
                id: node.id.clone(),
                name: node.name.clone(),
                definition: bounded_json(&node.definition, 64 * 1024)?,
                depends_on: serde_json::to_string(&node.depends_on)
                    .map_err(|error| BackendError::Internal(error.to_string()))?,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    if nodes
        .iter()
        .map(|node| node.definition.len() + node.depends_on.len())
        .sum::<usize>()
        > 512 * 1024
    {
        return Err(invalid("节点快照合计超过允许大小"));
    }
    Ok(EncodedProductJob {
        definition_snapshot,
        parameters,
        nodes,
    })
}

fn validate_dag(nodes: &[ProductJobNodeDefinition]) -> Result<(), BackendError> {
    let mut dependencies = HashMap::new();
    for node in nodes {
        if dependencies
            .insert(node.id.as_str(), node.depends_on.as_slice())
            .is_some()
        {
            return Err(invalid("运行节点 ID 重复"));
        }
        let unique: HashSet<&String> = node.depends_on.iter().collect();
        if unique.len() != node.depends_on.len() {
            return Err(invalid("运行节点依赖重复"));
        }
    }
    let mut completed = HashSet::new();
    while completed.len() != nodes.len() {
        let before = completed.len();
        for node in nodes {
            if node
                .depends_on
                .iter()
                .any(|id| !dependencies.contains_key(id.as_str()))
            {
                return Err(invalid("运行节点依赖不存在"));
            }
            if node
                .depends_on
                .iter()
                .all(|id| completed.contains(id.as_str()))
            {
                completed.insert(node.id.as_str());
            }
        }
        if completed.len() == before {
            return Err(invalid("运行节点依赖形成环"));
        }
    }
    Ok(())
}

pub fn same_request(job: &ProductJob, params: &EnqueueProductJobParams) -> bool {
    job.actor_id == params.actor_id
        && job.source_kind == params.source_kind
        && job.source_id == params.source_id
        && job.source_version == params.source_version
        && job.action == params.action
        && job.definition_snapshot == params.definition_snapshot
        && job.parameters == params.parameters
        && job.can_retry == params.can_retry
        && job.nodes.len() == params.nodes.len()
        && params.nodes.iter().all(|wanted| {
            job.nodes.iter().any(|node| {
                node.id == wanted.id
                    && node.name == wanted.name
                    && node.definition == wanted.definition
                    && node.depends_on == wanted.depends_on
            })
        })
}

pub fn completion(
    outcome: ProductJobOutcome,
    result: &Option<Value>,
    error: &Option<String>,
) -> Result<(Option<String>, Option<String>), BackendError> {
    if let Some(error) = error {
        bounded_text(error, 4000)?;
    }
    match outcome {
        ProductJobOutcome::Succeeded if error.is_some() => {
            return Err(invalid("成功运行不能包含错误"));
        },
        ProductJobOutcome::Failed if error.is_none() => {
            return Err(invalid("失败运行必须包含实际错误"));
        },
        ProductJobOutcome::Cancelled if result.is_some() => {
            return Err(invalid("取消运行不能保存成功结果"));
        },
        _ => {},
    }
    let result = result
        .as_ref()
        .map(|value| bounded_json(value, 256 * 1024))
        .transpose()?;
    Ok((result, error.clone()))
}

pub fn outcome_text(outcome: ProductJobOutcome) -> &'static str {
    match outcome {
        ProductJobOutcome::Succeeded => "succeeded",
        ProductJobOutcome::Failed => "failed",
        ProductJobOutcome::Cancelled => "cancelled",
    }
}

pub fn check_node_result_budget(
    job: &ProductJob,
    node_id: &str,
    result: &Option<Value>,
) -> Result<(), BackendError> {
    let mut total = result
        .as_ref()
        .map(|value| bounded_json(value, 256 * 1024))
        .transpose()?
        .map_or(0, |value| value.len());
    for node in &job.nodes {
        if node.id == node_id {
            continue;
        }
        if let Some(value) = &node.result {
            total += bounded_json(value, 256 * 1024)?.len();
        }
    }
    if total > 256 * 1024 {
        return Err(invalid("运行节点结果合计超过允许大小"));
    }
    Ok(())
}

pub fn check_job_completion(
    job: &ProductJob,
    outcome: ProductJobOutcome,
) -> Result<(), BackendError> {
    if job
        .nodes
        .iter()
        .any(|node| node.status == ProductJobNodeStatus::Running)
    {
        return Err(BackendError::Conflict("仍有正在执行的节点".into()));
    }
    match outcome {
        ProductJobOutcome::Succeeded
            if job.cancel_requested
                || job
                    .nodes
                    .iter()
                    .any(|node| node.status != ProductJobNodeStatus::Succeeded) =>
        {
            Err(BackendError::Conflict("节点未全部成功或已请求取消".into()))
        },
        ProductJobOutcome::Cancelled if !job.cancel_requested => {
            Err(BackendError::Conflict("运行未请求取消".into()))
        },
        _ => Ok(()),
    }
}
