use super::{
    QualityCheckResult, QualityCheckStatus, QualityNode, bad, predicate, snapshot::identifier,
};
use crate::{
    models::error::AppError,
    services::{
        product_domain::warehouse::canonical_data_type,
        query_svc::{QueryResult, validate_read_only_sql},
    },
};
use serde_json::Value;
use std::collections::HashSet;

pub fn compile_node(value: &Value) -> Result<String, AppError> {
    let node: QualityNode =
        serde_json::from_value(value.clone()).map_err(|_| bad("质量节点不符合冻结定义合同"))?;
    if node.format_version != 1
        || node.node_type != "quality_check"
        || node.rule_version < 1
        || node.columns.is_empty()
        || node.columns.len() > 1000
        || node
            .sample_limit
            .is_some_and(|limit| limit == 0 || limit > 1_000_000)
    {
        return Err(bad("质量节点版本或字段范围无效"));
    }
    let mut names = HashSet::new();
    for column in &node.columns {
        identifier(&column.name)?;
        if !names.insert(column.name.to_ascii_lowercase())
            || canonical_data_type(&column.data_type)? != column.data_type
        {
            return Err(bad("冻结字段名或标准化类型无效"));
        }
    }
    let target = node
        .columns
        .iter()
        .find(|column| column.name == node.column_name)
        .ok_or_else(|| bad("规则目标字段不在冻结元数据范围"))?;
    let parts: Vec<_> = node.table_name.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(bad(
            "质量物理对象必须是明确的schema.table或catalog.schema.table",
        ));
    }
    for part in &parts {
        identifier(part)?;
    }
    let table = parts
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(".");
    let projection = node
        .columns
        .iter()
        .map(|column| format!("\"{}\"", column.name))
        .collect::<Vec<_>>()
        .join(",");
    let limit = node
        .sample_limit
        .map(|limit| format!(" LIMIT {limit}"))
        .unwrap_or_default();
    let source = format!("WITH \"__quality_base\" AS (SELECT {projection} FROM {table}{limit}) ");
    let column = format!("\"{}\"", target.name);
    let sql = if node.rule_type == "unique" {
        if node.conditions != serde_json::json!([]) {
            return Err(bad("unique规则conditions必须为空数组，不会忽略额外条件"));
        }
        format!(
            "{source}SELECT COUNT(*) AS checked_rows, COALESCE(SUM(CASE WHEN \"__quality_value\" IS NULL OR \"__quality_occurrences\">1 THEN 1 ELSE 0 END),0) AS failed_rows FROM (SELECT {column} AS \"__quality_value\", COUNT(*) OVER(PARTITION BY {column}) AS \"__quality_occurrences\" FROM \"__quality_base\") AS \"__quality_duplicates\""
        )
    } else {
        let passing = match node.rule_type.as_str() {
            "not_null" => {
                if node.conditions != serde_json::json!([]) {
                    return Err(bad("not_null规则conditions必须为空数组，不会忽略额外条件"));
                }
                format!("{column} IS NOT NULL")
            },
            "range" | "expression" => predicate::compile(
                &node.rule_type,
                &node.conditions,
                &node.columns,
                &target.name,
            )?,
            _ => return Err(bad("质量规则类型不受支持")),
        };
        format!(
            "{source}SELECT COUNT(*) AS checked_rows, COALESCE(SUM(CASE WHEN COALESCE(({passing}),FALSE) THEN 0 ELSE 1 END),0) AS failed_rows FROM \"__quality_base\""
        )
    };
    validate_read_only_sql(&sql)?;
    Ok(sql)
}

pub fn map_node_result(definition: &Value, result: QueryResult) -> Result<Value, AppError> {
    compile_node(definition)?;
    if result.rows.len() != 1 || result.row_count != 1 {
        return Err(bad("质量COUNT查询必须实际返回一条聚合记录"));
    }
    let count = |key: &str| -> Result<u64, AppError> {
        let value = &result.rows[0][key];
        value
            .as_u64()
            .or_else(|| {
                value
                    .as_str()
                    .filter(|text| {
                        !text.is_empty()
                            && text.len() <= 20
                            && text.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .and_then(|text| text.parse().ok())
            })
            .ok_or_else(|| bad("质量COUNT查询缺少有效非负整数计数"))
    };
    let checked_rows = count("checked_rows")?;
    let failed_rows = count("failed_rows")?;
    let node: QualityNode =
        serde_json::from_value(definition.clone()).map_err(|_| bad("质量节点格式无效"))?;
    if failed_rows > checked_rows
        || node
            .sample_limit
            .is_some_and(|limit| checked_rows > u64::from(limit))
    {
        return Err(bad("质量聚合计数与实际规则或采样范围不一致"));
    }
    let status = if checked_rows == 0 {
        QualityCheckStatus::EmptyData
    } else if failed_rows == 0 {
        QualityCheckStatus::Passed
    } else {
        QualityCheckStatus::Failed
    };
    serde_json::to_value(QualityCheckResult {
        rule_id: node.rule_id,
        rule_version: node.rule_version,
        quality_object_id: node.quality_object_id,
        metadata_object_id: node.metadata_object_id,
        checked_rows,
        failed_rows,
        failure_ratio: if checked_rows == 0 {
            None
        } else {
            Some(failed_rows as f64 / checked_rows as f64)
        },
        status,
        evaluation_scope: if node.sample_limit.is_some() {
            "sample"
        } else {
            "full_table"
        }
        .into(),
    })
    .map_err(|_| bad("质量结果无法编码"))
}
