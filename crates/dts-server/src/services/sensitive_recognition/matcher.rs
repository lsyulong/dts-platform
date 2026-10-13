// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, compiler, types::RecognitionSnapshot};
use crate::{
    models::error::AppError,
    services::{data_access::AuthorizedQuery, query_svc::QueryResult},
};
use dts_backend::repo::{product_job::ProductJob, sensitive_findings::*};
use serde_json::Value;

pub(crate) fn scan(
    job: &ProductJob,
    raw: QueryResult,
    authorized: &AuthorizedQuery,
) -> Result<SensitiveScanResult, AppError> {
    let sql = compiler::prepare_claimed(job)?.remove(0).1;
    if !authorized.is_controlled_sensitive_scan()
        || authorized.actor_scope() != (&*job.tenant_id, &*job.actor_id)
        || authorized.sql() != sql
    {
        return Err(AppError::Forbidden(
            "扫描输出缺少与实际租约节点绑定的可信查询授权".into(),
        ));
    }
    let snapshot: RecognitionSnapshot =
        serde_json::from_value(job.definition_snapshot.clone()).map_err(|_| bad("识别快照无效"))?;
    evaluate(&snapshot, raw)
}
pub(super) fn evaluate(
    snapshot: &RecognitionSnapshot,
    raw: QueryResult,
) -> Result<SensitiveScanResult, AppError> {
    compiler::validate_snapshot(snapshot)?;
    let (rule, regex) = compiler::rule(&snapshot.rule.properties)?;
    let task = compiler::task(&snapshot.properties)?;
    let columns = compiler::selected(snapshot)?;
    if raw.row_count != raw.rows.len()
        || raw.rows.len() > task.sample_limit as usize
        || raw.columns.len() != columns.len()
        || (rule.match_target == "column_name" && !raw.rows.is_empty())
        || serde_json::to_vec(&raw)
            .map_err(|_| bad("真实识别结果无法编码"))?
            .len()
            > 2 * 1024 * 1024
    {
        return Err(bad("取样结果超过限制或真实结构不完整，未保存部分识别结果"));
    }
    let mut evidence = Vec::with_capacity(columns.len());
    let mut bytes = 0usize;
    for (actual, column) in raw.columns.iter().zip(&columns) {
        if actual.name != column.name || !actual_type(&column.data_type, &actual.data_type) {
            return Err(bad("真实取样字段或类型与冻结元数据不一致"));
        }
        let mut found = SensitiveColumnEvidence {
            column_name: column.name.clone(),
            data_type: column.data_type.clone(),
            checked_values: 0,
            matched_values: 0,
            null_values: 0,
        };
        if rule.match_target == "column_name" {
            found.checked_values = 1;
            found.matched_values = u32::from(regex.is_match(&column.name));
        } else {
            for row in &raw.rows {
                let value = row
                    .as_object()
                    .filter(|r| {
                        r.len() == columns.len()
                            && r.keys().all(|name| columns.iter().any(|c| &c.name == name))
                    })
                    .and_then(|r| r.get(&column.name))
                    .ok_or_else(|| bad("真实样本缺少字段或含额外字段"))?;
                if value.is_null() {
                    if !column.nullable {
                        return Err(bad("真实样本违反冻结非空约束"));
                    }
                    found.null_values += 1;
                    continue;
                }
                let text = scalar(
                    value,
                    compiler::source_family(&column.data_type)
                        .ok_or_else(|| bad("样本来源类型不受支持"))?,
                )?;
                bytes = bytes
                    .checked_add(text.len())
                    .ok_or_else(|| bad("样本字节成本溢出"))?;
                if text.len() > 4096 || bytes > 1024 * 1024 {
                    return Err(bad(
                        "敏感识别样本单值超过4096字节或总计超过1MiB，未截断匹配",
                    ));
                }
                found.checked_values += 1;
                found.matched_values += u32::from(regex.is_match(&text));
            }
        }
        evidence.push(found);
    }
    Ok(SensitiveScanResult {
        evaluation_scope: if rule.match_target == "column_name" {
            "column_names"
        } else {
            "sample"
        }
        .into(),
        sample_limit: task.sample_limit,
        sampled_rows: raw.rows.len() as u32,
        columns: evidence,
        definition_hash: snapshot_hash(&serde_json::json!(snapshot))?,
        producer_committed: false,
        finding_ids: vec![],
    })
}
fn scalar(value: &Value, family: &str) -> Result<String, AppError> {
    match (family, value) {
        ("text" | "date", Value::String(v)) => Ok(v.clone()),
        ("number", Value::Number(v)) => Ok(v.to_string()),
        ("number", Value::String(v))
            if !v.is_empty()
                && v.bytes()
                    .all(|c| c.is_ascii_digit() || b"-+.eE".contains(&c))
                && v.parse::<f64>().is_ok_and(|n| n.is_finite()) =>
        {
            Ok(v.clone())
        },
        ("boolean", Value::Bool(v)) => Ok(v.to_string()),
        _ => Err(bad("实际样本值与受支持的冻结字段类型不一致")),
    }
}
fn actual_type(source: &str, actual: &str) -> bool {
    let source = source.to_ascii_uppercase().replace(' ', "");
    let actual = actual.to_ascii_uppercase().replace(' ', "");
    if source == actual {
        return true;
    }
    if source.starts_with("DECIMAL(") {
        return actual
            .strip_prefix("DECIMAL128")
            .or_else(|| actual.strip_prefix("DECIMAL256"))
            .is_some_and(|v| v == &source[7..]);
    }
    match compiler::source_family(&source) {
        Some("text") => matches!(actual.as_str(), "UTF8" | "LARGEUTF8" | "UTF8VIEW"),
        Some("boolean") => actual == "BOOLEAN",
        Some("date") => match source.as_str() {
            "DATE" => actual == "DATE32",
            "TIMESTAMP" => actual == "TIMESTAMP(MICROSECOND,NONE)",
            "TIMESTAMP_S" => actual == "TIMESTAMP(SECOND,NONE)",
            "TIMESTAMP_MS" => actual == "TIMESTAMP(MILLISECOND,NONE)",
            "TIMESTAMP_NS" => actual == "TIMESTAMP(NANOSECOND,NONE)",
            _ => false,
        },
        Some("number") => matches!(
            (source.as_str(), actual.as_str()),
            ("TINYINT", "INT8")
                | ("SMALLINT", "INT16")
                | ("INTEGER" | "INT", "INT32")
                | ("BIGINT", "INT64")
                | ("UTINYINT", "UINT8")
                | ("USMALLINT", "UINT16")
                | ("UINTEGER", "UINT32")
                | ("UBIGINT", "UINT64")
                | ("FLOAT" | "REAL", "FLOAT32")
                | ("DOUBLE", "FLOAT64")
        ),
        None => false,
        _ => false,
    }
}
