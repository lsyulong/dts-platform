use super::{bad, compiler};
use crate::{models::error::AppError, services::query_svc::QueryResult};
use dts_backend::repo::product_job::ProductJob;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn map_node_result(job: &ProductJob, result: QueryResult) -> Result<Value, AppError> {
    let snapshot = compiler::snapshot(job)?;
    let plan = snapshot
        .publication
        .plan
        .as_ref()
        .ok_or_else(|| bad("指标计划缺失"))?;
    let expected = plan
        .dimensions
        .iter()
        .cloned()
        .chain(["metric_value".into(), "input_rows".into()])
        .collect::<Vec<String>>();
    if result.row_count != result.rows.len()
        || result.rows.len() > snapshot.request.row_limit as usize
        || result.columns.len() != expected.len()
        || result
            .columns
            .iter()
            .zip(&expected)
            .any(|(column, name)| &column.name != name)
    {
        return Err(bad("指标实际结果字段不匹配或超过限制，请缩小分组范围"));
    }
    if !result.columns[result.columns.len() - 2].is_numeric
        || !result.columns[result.columns.len() - 1].is_numeric
        || !matches!(
            result.columns[result.columns.len() - 1].data_type.as_str(),
            "Int64" | "UInt64" | "BIGINT" | "UBIGINT"
        )
    {
        return Err(bad("指标实际度量或COUNT输出类型不是原生数值口径"));
    }
    if plan.numeric_semantics == "engine_native_exact"
        && matches!(
            result.columns[result.columns.len() - 2].data_type.as_str(),
            "Float32" | "Float64" | "FLOAT" | "DOUBLE"
        )
    {
        return Err(bad(
            "实际浮点结果与已发布精确指标口径不一致，请重新核对物理字段",
        ));
    }
    let mut groups = BTreeSet::new();
    let mut states = vec![];
    for row in &result.rows {
        let object = row.as_object().ok_or_else(|| bad("指标实际结果不是对象"))?;
        if object.len() != expected.len() || expected.iter().any(|name| !object.contains_key(name))
        {
            return Err(bad("指标实际结果缺少冻结字段"));
        }
        let count = super::precision::unsigned_count(&row["input_rows"])
            .ok_or_else(|| bad("指标实际输入行数不是非负整数或精确整数文本"))?;
        if !super::precision::numeric_value(
            &row["input_rows"],
            &result.columns[result.columns.len() - 1],
        ) {
            return Err(bad("指标输入行数超出实际COUNT字段类型"));
        }
        let value = &row["metric_value"];
        let value_column = &result.columns[result.columns.len() - 2];
        if !value.is_null() && !super::precision::numeric_value(value, value_column) {
            return Err(bad("实际指标值不符合原生数值类型、精确Decimal文本或NULL"));
        }
        if !groups.insert(
            serde_json::to_string(
                &plan
                    .dimensions
                    .iter()
                    .map(|name| &row[name])
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| bad("指标分组不能编码"))?,
        ) {
            return Err(bad("实际指标返回重复分组"));
        }
        states.push(if count == 0 {
            "empty_data"
        } else if value.is_null() {
            "undefined_value"
        } else {
            "calculated"
        });
    }
    Ok(
        json!({"columns":result.columns,"rows":result.rows,"row_count":result.row_count,"suggested_chart":null,
        "result_states":states,"metric_id":snapshot.source_id,"metric_kind":snapshot.source_kind,"metric_version":snapshot.source_version,
        "metadata_id":snapshot.metadata.id,"metadata_version":snapshot.metadata.version,"model_id":plan.model_id,"model_version":plan.model_version,
        "grain_columns":plan.grain,"dimensions":plan.dimensions,"unit":plan.unit,"window":snapshot.window,"executor":"metric_aggregate_v1","numeric_semantics":plan.numeric_semantics,"complete":true,
        "dependency_versions":snapshot.publication.references.iter().map(|reference|json!({"kind":reference.kind,"id":reference.id,"version":reference.version})).collect::<Vec<_>>() }),
    )
}
