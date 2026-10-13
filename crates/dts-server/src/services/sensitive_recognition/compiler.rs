// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, types::*};
use crate::{models::error::AppError, services::query_svc::validate_read_only_sql};
use dts_backend::repo::{metadata_catalog::binding_matches, product_job::*};
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};

pub(super) fn classification(properties: &Value) -> Result<ClassificationDefinition, AppError> {
    let value: ClassificationDefinition = serde_json::from_value(properties.clone())
        .map_err(|_| bad("密级要求整数rank和handling_policy"))?;
    if !(1..=999).contains(&value.rank)
        || value.handling_policy.trim().is_empty()
        || value.handling_policy.len() > 4096
    {
        return Err(bad("密级顺序须1..999，处理要求1..4096字节"));
    }
    Ok(value)
}
pub(super) fn rule(properties: &Value) -> Result<(RecognitionRule, Regex), AppError> {
    let value: RecognitionRule = serde_json::from_value(definition_properties(properties))
        .map_err(|_| bad("识别规则字段不符合固定合同"))?;
    if value.classification_id.is_empty()
        || value.classification_id.len() > 128
        || !matches!(value.match_target.as_str(), "column_name" | "sample_value")
        || value.pattern.is_empty()
        || value.pattern.len() > 1024
        || !(1..=1000).contains(&value.sample_limit)
        || value.source_types.is_empty()
        || value.source_types.len() > 4
        || value
            .source_types
            .iter()
            .any(|t| !matches!(t.as_str(), "text" | "number" | "date" | "boolean"))
        || value
            .source_types
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != value.source_types.len()
    {
        return Err(bad("识别表达式、匹配目标、来源类型或成本上限无效"));
    }
    let regex = RegexBuilder::new(&value.pattern)
        .size_limit(1024 * 1024)
        .dfa_size_limit(1024 * 1024)
        .nest_limit(32)
        .build()
        .map_err(|_| bad("识别正则无效或超出有界编译成本"))?;
    Ok((value, regex))
}
pub(super) fn task(properties: &Value) -> Result<RecognitionTask, AppError> {
    let value: RecognitionTask = serde_json::from_value(definition_properties(properties))
        .map_err(|_| bad("识别任务字段不符合固定合同"))?;
    if value.rule_id.is_empty()
        || value.object_id.is_empty()
        || value.rule_id.len() > 128
        || value.object_id.len() > 128
        || !(1..=1000).contains(&value.sample_limit)
    {
        return Err(bad("任务真实引用或取样上限无效"));
    }
    Ok(value)
}
fn definition_properties(properties: &Value) -> Value {
    let mut result = properties.clone();
    if let Some(object) = result.as_object_mut() {
        object.remove("publication");
    }
    result
}
pub(super) use dts_backend::repo::sensitive_findings::recognition_source_family as source_family;
pub(super) fn selected(
    snapshot: &RecognitionSnapshot,
) -> Result<Vec<dts_backend::repo::metadata_catalog::MetadataColumn>, AppError> {
    let (rule, _) = rule(&snapshot.rule.properties)?;
    let columns: Vec<_> = snapshot
        .metadata
        .columns
        .iter()
        .filter(|c| {
            source_family(&c.data_type).is_some_and(|family| {
                rule.match_target == "column_name" || rule.source_types.iter().any(|v| v == family)
            })
        })
        .cloned()
        .collect();
    if columns.is_empty() || columns.len() > 128 {
        return Err(bad("没有符合规则来源类型的字段或超过128列上限"));
    }
    Ok(columns)
}
pub(super) fn validate_snapshot(snapshot: &RecognitionSnapshot) -> Result<(), AppError> {
    let task = task(&snapshot.properties)?;
    let (rule, _) = rule(&snapshot.rule.properties)?;
    classification(&snapshot.classification.properties)?;
    let columns: Vec<dts_backend::repo::metadata_catalog::MetadataColumn> =
        serde_json::from_value(snapshot.metadata.properties["columns"].clone())
            .map_err(|_| bad("识别真实字段快照无效"))?;
    if snapshot.format_version != 1
        || snapshot.executor != "sensitive_scan_v1"
        || snapshot.source_kind != "recognition_task"
        || snapshot.source_version < 1
        || snapshot.task.id != snapshot.source_id
        || snapshot.task.version != snapshot.source_version
        || snapshot.task.properties != snapshot.properties
        || task.rule_id != snapshot.rule.id
        || task.object_id != snapshot.metadata.id
        || rule.classification_id != snapshot.classification.id
        || task.sample_limit > rule.sample_limit
        || snapshot.metadata.binding.engine != "swanlake"
        || snapshot.metadata.binding.tenant_id != snapshot.tenant_id
        || snapshot.metadata.binding.object_version != snapshot.metadata.version
        || !binding_matches(&snapshot.metadata.binding, &snapshot.metadata.properties)
        || columns != snapshot.metadata.columns
        || columns.is_empty()
        || columns.len() > 128
        || [
            snapshot.rule.version,
            snapshot.classification.version,
            snapshot.metadata.version,
        ]
        .iter()
        .any(|v| *v < 1)
    {
        return Err(bad("识别快照身份、版本、实际元数据证明或取样成本不一致"));
    }
    selected(snapshot)?;
    Ok(())
}
fn quote(value: &str) -> Result<String, AppError> {
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(char::is_control)
        || value.contains(['"', '\\'])
    {
        return Err(bad("实际来源标识不受支持"));
    }
    Ok(format!("\"{value}\""))
}
pub(super) fn sql(snapshot: &RecognitionSnapshot) -> Result<String, AppError> {
    validate_snapshot(snapshot)?;
    let columns = selected(snapshot)?;
    let (rule, _) = rule(&snapshot.rule.properties)?;
    let binding = &snapshot.metadata.binding;
    let table = [
        &binding.table_catalog,
        &binding.schema_name,
        &binding.table_name,
    ]
    .into_iter()
    .map(|v| quote(v))
    .collect::<Result<Vec<_>, _>>()?
    .join(".");
    let fields = columns
        .iter()
        .map(|v| quote(&v.name))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let limit = if rule.match_target == "column_name" {
        0
    } else {
        task(&snapshot.properties)?.sample_limit
    };
    let sql = format!("SELECT {fields} FROM {table} LIMIT {limit}");
    validate_read_only_sql(&sql)?;
    Ok(sql)
}
pub(super) fn node(snapshot: &RecognitionSnapshot) -> Result<ProductJobNodeDefinition, AppError> {
    Ok(ProductJobNodeDefinition {
        id: "recognition".into(),
        name: "真实有界敏感识别".into(),
        definition: json!({"format_version":1,"type":"sensitive_scan","definition_hash":dts_backend::repo::sensitive_findings::snapshot_hash(&json!(snapshot))?}),
        depends_on: vec![],
    })
}
pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let snapshot: RecognitionSnapshot = serde_json::from_value(job.definition_snapshot.clone())
        .map_err(|_| bad("识别运行快照无效"))?;
    validate_snapshot(&snapshot)?;
    let expected = node(&snapshot)?;
    if snapshot.task.properties["publication"]
        != json!({"format_version":1,"rule":snapshot.rule,"classification":snapshot.classification,"metadata":snapshot.metadata})
        || snapshot.rule.properties["publication"]
            != json!({"format_version":1,"classification":snapshot.classification})
    {
        return Err(bad("敏感识别任务没有由发布服务冻结的实际依赖版本"));
    }
    if job.source_kind != "recognition_task"
        || job.action != "execute_recognition"
        || job.tenant_id != snapshot.tenant_id
        || job.source_id != snapshot.source_id
        || job.source_version != snapshot.source_version
        || !job.can_retry
        || job.parameters != json!({})
        || job.nodes.len() != 1
        || job.nodes[0].id != expected.id
        || job.nodes[0].name != expected.name
        || job.nodes[0].definition != expected.definition
        || !job.nodes[0].depends_on.is_empty()
    {
        return Err(bad("识别真实任务/节点与可信编译器不一致"));
    }
    Ok(vec![(expected.id, sql(&snapshot)?)])
}
