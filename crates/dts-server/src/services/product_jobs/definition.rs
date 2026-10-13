// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use dts_backend::{
    ProductResource,
    repo::product_job::{ProductJob, ProductJobNodeDefinition},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::models::error::AppError;

use super::parameters::{self, ParameterDefinition, bad};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowDefinition {
    nodes: Vec<WorkflowNode>,
    #[serde(default)]
    parameters: Vec<ParameterDefinition>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowNode {
    id: String,
    name: String,
    sql: String,
    depends_on: Vec<String>,
}

pub(super) struct ExecutionDefinition {
    nodes: Vec<WorkflowNode>,
    parameters: Vec<ParameterDefinition>,
}

pub(super) struct PreparedJob {
    pub nodes: Vec<ProductJobNodeDefinition>,
    pub parameters: Value,
    pub queries: Vec<(String, String)>,
}

#[derive(Serialize)]
pub struct DefinitionValidation {
    pub valid: bool,
    pub execution_mode: &'static str,
    pub node_count: usize,
    pub can_retry: bool,
    pub binding_validated: bool,
    pub schedule_registered: bool,
    pub schedule_definition: Option<super::WorkflowScheduleDefinition>,
}

pub fn validate_definition(
    kind: &str,
    properties: &Value,
) -> Result<DefinitionValidation, AppError> {
    let definition = parse(kind, properties)?;
    Ok(DefinitionValidation {
        valid: true,
        execution_mode: "read_only",
        node_count: definition.nodes.len(),
        can_retry: true,
        binding_validated: false,
        schedule_registered: false,
        schedule_definition: if kind == "development_workflow" {
            super::schedule::validate(properties)?
        } else {
            None
        },
    })
}

fn parse(kind: &str, properties: &Value) -> Result<ExecutionDefinition, AppError> {
    super::ProductJobService::validate_schedule_properties(kind, properties)?;
    let definition = match kind {
        "sql_document" => {
            let sql = properties
                .get("sql")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("SQL 文档缺少 sql"))?;
            let parameters = properties
                .get("parameters")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| json!([]));
            ExecutionDefinition {
                nodes: vec![WorkflowNode {
                    id: "sql".into(),
                    name: "SQL 查询".into(),
                    sql: sql.into(),
                    depends_on: vec![],
                }],
                parameters: serde_json::from_value(parameters)
                    .map_err(|_| bad("SQL 参数定义不符合合同"))?,
            }
        },
        "development_workflow" => {
            let source = properties
                .get("definition")
                .cloned()
                .ok_or_else(|| bad("工作流缺少 definition"))?;
            let workflow: WorkflowDefinition = serde_json::from_value(source)
                .map_err(|_| bad("工作流 definition 必须为真实节点与可选类型化参数"))?;
            ExecutionDefinition {
                nodes: workflow.nodes,
                parameters: workflow.parameters,
            }
        },
        _ => return Err(bad("当前执行器仅支持 SQL 文档和只读开发工作流")),
    };
    parameters::validate_definitions(&definition.parameters)?;
    validate_nodes(&definition)?;
    Ok(definition)
}

fn validate_nodes(definition: &ExecutionDefinition) -> Result<(), AppError> {
    if definition.nodes.is_empty() || definition.nodes.len() > 64 {
        return Err(bad("工作流必须包含 1 至 64 个节点"));
    }
    let mut ids = BTreeSet::new();
    let mut used_parameters = BTreeSet::new();
    for node in &definition.nodes {
        if !parameters::identifier(&node.id)
            || !ids.insert(&node.id)
            || node.name.trim().is_empty()
            || node.name.len() > 256
        {
            return Err(bad("工作流节点 ID 或名称无效，或 ID 重复"));
        }
        let unique: BTreeSet<_> = node.depends_on.iter().collect();
        if unique.len() != node.depends_on.len() {
            return Err(bad("工作流节点依赖重复"));
        }
        used_parameters.extend(parameters::inspect(&node.sql, &definition.parameters)?);
    }
    if definition
        .parameters
        .iter()
        .any(|parameter| !used_parameters.contains(&parameter.name))
    {
        return Err(bad("存在没有被任何 SQL 使用的参数定义"));
    }
    let mut completed = BTreeSet::new();
    while completed.len() < definition.nodes.len() {
        let before = completed.len();
        for node in &definition.nodes {
            if node.depends_on.iter().any(|id| !ids.contains(id)) {
                return Err(bad("工作流引用了不存在的依赖节点"));
            }
            if node.depends_on.iter().all(|id| completed.contains(id)) {
                completed.insert(&node.id);
            }
        }
        if completed.len() == before {
            return Err(bad("工作流依赖存在环"));
        }
    }
    Ok(())
}

fn prepare(kind: &str, properties: &Value, values: &Value) -> Result<PreparedJob, AppError> {
    let definition = parse(kind, properties)?;
    let (parameters, expressions) = parameters::resolve(&definition.parameters, values)?;
    let mut nodes = Vec::new();
    let mut queries = Vec::new();
    for node in definition.nodes {
        let sql = parameters::bind(&node.sql, &expressions)?;
        let snapshot = json!({"source_sql":node.sql});
        if serde_json::to_vec(&snapshot)
            .map_err(|_| bad("节点 SQL 无法编码"))?
            .len()
            > 64 * 1024
        {
            return Err(bad("节点 SQL 快照超过 64 KiB"));
        }
        queries.push((node.id.clone(), sql));
        nodes.push(ProductJobNodeDefinition {
            id: node.id,
            name: node.name,
            definition: snapshot,
            depends_on: node.depends_on,
        });
    }
    Ok(PreparedJob {
        nodes,
        parameters,
        queries,
    })
}

pub(super) fn prepare_resource(
    resource: &ProductResource,
    values: &Value,
) -> Result<PreparedJob, AppError> {
    prepare(&resource.kind, &resource.properties, values)
}

pub(super) fn snapshot(resource: &ProductResource) -> Value {
    json!({"format_version":1,"tenant_id":resource.tenant_id,"source_kind":resource.kind,"source_id":resource.id,"source_version":resource.version,"properties":resource.properties})
}

pub(super) fn prepare_claimed(job: &ProductJob) -> Result<PreparedJob, AppError> {
    let source = &job.definition_snapshot;
    if !job.can_retry
        || job.action != "execute_read_only"
        || source.get("format_version").and_then(Value::as_i64) != Some(1)
        || source.get("tenant_id").and_then(Value::as_str) != Some(job.tenant_id.as_str())
        || source.get("source_kind").and_then(Value::as_str) != Some(job.source_kind.as_str())
        || source.get("source_id").and_then(Value::as_str) != Some(job.source_id.as_str())
        || source.get("source_version").and_then(Value::as_i64) != Some(job.source_version)
    {
        return Err(bad("运行快照与来源身份不一致"));
    }
    let prepared = prepare(
        &job.source_kind,
        source
            .get("properties")
            .ok_or_else(|| bad("运行缺少属性快照"))?,
        &job.parameters,
    )?;
    if prepared.nodes.len() != job.nodes.len()
        || prepared.nodes.iter().any(|node| {
            !job.nodes.iter().any(|actual| {
                node.id == actual.id
                    && node.name == actual.name
                    && node.definition == actual.definition
                    && node.depends_on == actual.depends_on
            })
        })
    {
        return Err(bad("节点执行快照与受控定义不一致"));
    }
    Ok(prepared)
}
