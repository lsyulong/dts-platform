use super::{FrozenColumn, QualityNode, QualitySnapshot, bad, compile_node, ids, text};
use crate::{models::error::AppError, services::product_domain::warehouse::canonical_data_type};
use dts_backend::repo::product_job::*;
use serde_json::Value;
use std::collections::HashSet;

pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let snapshot: QualitySnapshot = serde_json::from_value(job.definition_snapshot.clone())
        .map_err(|_| bad("质量快照格式不符合合同"))?;
    if !job.can_retry
        || job.source_kind != "quality_task"
        || job.action != "execute_quality"
        || job.parameters != serde_json::json!({})
        || snapshot.tenant_id != job.tenant_id
        || snapshot.source_kind != job.source_kind
        || snapshot.source_id != job.source_id
        || snapshot.source_version != job.source_version
    {
        return Err(bad("质量运行身份与只读执行快照不一致"));
    }
    validate_snapshot(&snapshot, true)?;
    let nodes = nodes(&snapshot)?;
    if nodes.len() != job.nodes.len()
        || nodes.iter().any(|expected| {
            !job.nodes.iter().any(|actual| {
                expected.id == actual.id
                    && expected.name == actual.name
                    && expected.definition == actual.definition
                    && expected.depends_on == actual.depends_on
            })
        })
    {
        return Err(bad("质量节点与冻结的真实定义不一致"));
    }
    nodes
        .into_iter()
        .map(|node| Ok((node.id, compile_node(&node.definition)?)))
        .collect()
}

pub(super) fn engine_bound(snapshot: &QualitySnapshot) -> bool {
    snapshot
        .metadata
        .properties
        .get("engine_binding")
        .is_some_and(|binding| {
            binding["engine"] == "swanlake" && binding["tenant_id"] == snapshot.tenant_id
        })
}

pub(super) fn validate_snapshot(
    snapshot: &QualitySnapshot,
    execution: bool,
) -> Result<(), AppError> {
    if snapshot.format_version != 1
        || snapshot.executor != "quality_count_v1"
        || snapshot.source_version < 1
        || snapshot.task.id != snapshot.source_id
        || snapshot.task.version != snapshot.source_version
        || snapshot.task.properties != snapshot.properties
        || snapshot.object.version < 1
        || snapshot.metadata.version < 1
        || text(&snapshot.object.properties, "metadata_object_id")? != snapshot.metadata.id
    {
        return Err(bad("质量快照版本、对象或来源身份无效"));
    }
    if execution && (!engine_bound(snapshot) || snapshot.source_kind != "quality_task") {
        return Err(bad("元数据尚未由真实采集绑定当前租户的 SwanLake 分析引擎"));
    }
    let columns = columns(&snapshot.metadata.properties)?;
    selected_columns(&snapshot.object.properties, &columns)?;
    table_name(&snapshot.metadata.properties)?;
    if snapshot.source_kind == "quality_task" {
        if text(&snapshot.properties, "object_id")? != snapshot.object.id {
            return Err(bad("任务质量对象与快照不一致"));
        }
        let rule_ids = ids(&snapshot.properties, "rule_ids", true)?;
        if rule_ids.len() != snapshot.rules.len()
            || rule_ids
                .iter()
                .zip(&snapshot.rules)
                .any(|(id, rule)| id != &rule.id)
        {
            return Err(bad("质量规则ID列表与冻结规则快照不一致"));
        }
        if let Some(schedule) = snapshot
            .properties
            .get("schedule")
            .filter(|value| !value.is_null())
        {
            if !schedule.as_str().is_some_and(|value| value.is_empty()) {
                return Err(bad(
                    "质量执行当前只支持手动运行，定时计划尚未注册到真实调度器",
                ));
            }
        }
    }
    for rule in &snapshot.rules {
        if rule.version < 1 || text(&rule.properties, "object_id")? != snapshot.object.id {
            return Err(bad("规则不属于任务的质量对象"));
        }
    }
    nodes(snapshot)?;
    Ok(())
}

pub(super) fn table_name(properties: &Value) -> Result<String, AppError> {
    let table = text(properties, "table_name")?;
    identifier(table)?;
    let schema = properties
        .get("schema_name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    identifier(schema)?;
    match properties
        .get("table_catalog")
        .filter(|value| !value.is_null())
    {
        None => Ok(format!("{schema}.{table}")),
        Some(value) => {
            let catalog = value
                .as_str()
                .ok_or_else(|| bad("元数据table_catalog必须是实际库名"))?;
            identifier(catalog)?;
            Ok(format!("{catalog}.{schema}.{table}"))
        },
    }
}

pub(super) fn identifier(value: &str) -> Result<(), AppError> {
    let mut chars = value.bytes();
    if value.len() > 128
        || chars
            .next()
            .is_none_or(|byte| !byte.is_ascii_alphabetic() && byte != b'_')
        || chars.any(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
    {
        return Err(bad("质量SQL标识符只能使用字母、数字和下划线且不能数字开头"));
    }
    Ok(())
}

fn columns(properties: &Value) -> Result<Vec<FrozenColumn>, AppError> {
    let values = properties
        .get("columns")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 1000)
        .ok_or_else(|| bad("元数据必须包含1至1000个真实采集字段"))?;
    let mut names = HashSet::new();
    values
        .iter()
        .map(|column| {
            let name = text(column, "name")?;
            identifier(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(bad("元数据字段名重复"));
            }
            let nullable = match column.get("nullable") {
                None => true,
                Some(Value::Bool(value)) => *value,
                _ => return Err(bad("元数据nullable必须是布尔值")),
            };
            Ok(FrozenColumn {
                name: name.into(),
                data_type: canonical_data_type(text(column, "data_type")?)?,
                nullable,
            })
        })
        .collect()
}

fn selected_columns(
    properties: &Value,
    columns: &[FrozenColumn],
) -> Result<Vec<FrozenColumn>, AppError> {
    let Some(values) = properties.get("columns").filter(|value| !value.is_null()) else {
        return Ok(columns.to_vec());
    };
    let values = values
        .as_array()
        .filter(|values| values.len() <= 1000)
        .ok_or_else(|| bad("质量对象字段必须是字符串数组"))?;
    if values.is_empty() {
        return Ok(columns.to_vec());
    }
    let mut names = HashSet::new();
    values
        .iter()
        .map(|value| {
            let name = value
                .as_str()
                .ok_or_else(|| bad("质量对象字段必须是字符串数组"))?;
            identifier(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(bad("质量对象字段重复"));
            }
            columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(name))
                .cloned()
                .ok_or_else(|| bad("质量字段没有出现在冻结的元数据中"))
        })
        .collect()
}

pub(super) fn nodes(snapshot: &QualitySnapshot) -> Result<Vec<ProductJobNodeDefinition>, AppError> {
    let columns = selected_columns(
        &snapshot.object.properties,
        &columns(&snapshot.metadata.properties)?,
    )?;
    let table = table_name(&snapshot.metadata.properties)?;
    let sample_limit = snapshot
        .properties
        .get("sample_limit")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .filter(|limit| (1..=1_000_000).contains(limit))
                .map(|limit| limit as u32)
                .ok_or_else(|| bad("取样上限必须是1至1000000的整数"))
        })
        .transpose()?;
    let mut unique = HashSet::new();
    snapshot
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            if !unique.insert(&rule.id) {
                return Err(bad("冻结规则ID重复"));
            }
            let column_name = text(&rule.properties, "column_name")?;
            let column = columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(column_name))
                .ok_or_else(|| bad("规则字段不在质量对象的真实字段范围内"))?;
            let definition = serde_json::to_value(QualityNode {
                format_version: 1,
                node_type: "quality_check".into(),
                rule_index: index,
                rule_id: rule.id.clone(),
                rule_version: rule.version,
                quality_object_id: snapshot.object.id.clone(),
                metadata_object_id: snapshot.metadata.id.clone(),
                table_name: table.clone(),
                columns: columns.clone(),
                rule_type: text(&rule.properties, "rule_type")?.into(),
                column_name: column.name.clone(),
                conditions: rule
                    .properties
                    .get("conditions")
                    .cloned()
                    .ok_or_else(|| bad("规则缺少conditions"))?,
                sample_limit,
            })
            .map_err(|_| bad("质量节点无法编码"))?;
            compile_node(&definition)?;
            Ok(ProductJobNodeDefinition {
                id: format!("rule_{index}"),
                name: rule.name.clone(),
                definition,
                depends_on: vec![],
            })
        })
        .collect()
}
