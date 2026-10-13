use super::{
    bad,
    definition::{quote, text},
    expression,
    types::*,
    window,
};
use crate::{
    models::error::AppError,
    services::{
        product_domain::warehouse::canonical_data_type, product_jobs::validate_idempotency_key,
        query_svc::validate_read_only_sql,
    },
};
use chrono::{TimeZone, Timelike};
use dts_backend::repo::{
    metadata_catalog::binding_matches,
    product_job::{ProductJob, ProductJobNodeDefinition},
};
use serde_json::json;
use std::collections::BTreeMap;

fn time_literal(
    value: chrono::DateTime<chrono::Utc>,
    column_type: &str,
    timezone: &str,
) -> Result<String, AppError> {
    match canonical_data_type(column_type)?.as_str() {
        "TIMESTAMPTZ" => Ok(format!("CAST('{}' AS TIMESTAMPTZ)", value.to_rfc3339())),
        "DATE" | "TIMESTAMP" => {
            let tz: chrono_tz::Tz = timezone.parse().map_err(|_| bad("冻结时间字段时区无效"))?;
            let local = value.with_timezone(&tz).naive_local();
            if tz.from_local_datetime(&local).single().is_none() {
                return Err(bad(
                    "无时区物理时间的窗口端点处于夏令时歧义区间，请调整数据窗",
                ));
            }
            if column_type.eq_ignore_ascii_case("DATE") {
                if local.time().hour() != 0
                    || local.time().minute() != 0
                    || local.time().second() != 0
                    || local.time().nanosecond() != 0
                {
                    return Err(bad("DATE时间字段的数据窗须对齐业务时区午夜"));
                }
                Ok(format!("DATE '{}'", local.date()))
            } else {
                Ok(format!(
                    "TIMESTAMP '{}'",
                    local.format("%Y-%m-%d %H:%M:%S%.f")
                ))
            }
        },
        _ => Err(bad("指标时间字段类型不受支持")),
    }
}
fn aggregate(measure: &MetricMeasure) -> Result<String, AppError> {
    if !matches!(
        measure.aggregate.as_str(),
        "SUM" | "AVG" | "COUNT" | "MIN" | "MAX"
    ) {
        return Err(bad("冻结聚合函数不受支持"));
    }
    let argument = match (&measure.argument, &measure.modifier) {
        (Some(argument), Some(condition)) => {
            format!("CASE WHEN ({condition}) THEN ({argument}) ELSE NULL END")
        },
        (None, Some(condition)) if measure.aggregate == "COUNT" => {
            format!("CASE WHEN ({condition}) THEN 1 ELSE NULL END")
        },
        (Some(argument), None) => argument.clone(),
        (None, None) if measure.aggregate == "COUNT" => "*".into(),
        _ => return Err(bad("冻结聚合参数缺失")),
    };
    Ok(format!("{}({argument})", measure.aggregate))
}
pub(super) fn sql(snapshot: &MetricSnapshot) -> Result<String, AppError> {
    let plan = snapshot
        .publication
        .plan
        .as_ref()
        .ok_or_else(|| bad("此实体没有指标执行计划"))?;
    let binding = &snapshot.metadata.binding;
    let table = [
        &binding.table_catalog,
        &binding.schema_name,
        &binding.table_name,
    ]
    .iter()
    .map(|name| quote(name))
    .collect::<Result<Vec<_>, _>>()?
    .join(".");
    let time = expression::column(&plan.time_column, &snapshot.metadata.columns)?;
    let start = time_literal(snapshot.window.start, &time.data_type, &plan.time_timezone)?;
    let end = time_literal(snapshot.window.end, &time.data_type, &plan.time_timezone)?;
    let dimensions = plan
        .dimensions
        .iter()
        .map(|name| quote(name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut measures = BTreeMap::new();
    for measure in &plan.measures {
        text(&measure.code, 128)?;
        if measures
            .insert(measure.code.clone(), aggregate(measure)?)
            .is_some()
        {
            return Err(bad("冻结聚合编码重复"));
        }
    }
    if measures.is_empty() || measures.len() > 32 {
        return Err(bad("冻结聚合数量超出范围"));
    }
    let result = expression::substitute(&plan.expression, &measures)?;
    let mut projections = dimensions.clone();
    projections.push(format!("{result} AS \"metric_value\""));
    let grain = plan.grain.first().ok_or_else(|| bad("冻结粒度主键缺失"))?;
    if expression::column(grain, &snapshot.metadata.columns)?.nullable {
        return Err(bad("输入行计数需要实际非空粒度主键"));
    }
    projections.push(format!("COUNT({}) AS \"input_rows\"", quote(grain)?));
    let grouping = if dimensions.is_empty() {
        String::new()
    } else {
        format!(
            " GROUP BY {} ORDER BY {}",
            dimensions.join(","),
            dimensions.join(",")
        )
    };
    let sql = format!(
        "SELECT {} FROM {table} WHERE {} >= {start} AND {} < {end}{grouping} LIMIT {}",
        projections.join(","),
        quote(&time.name)?,
        quote(&time.name)?,
        snapshot.request.row_limit + 1
    );
    if sql.len() > 65536 {
        return Err(bad("指标编译SQL超过64KiB，请减少复合嵌套"));
    }
    validate_read_only_sql(&sql)?;
    Ok(sql)
}
pub(super) fn node(snapshot: &MetricSnapshot) -> Result<ProductJobNodeDefinition, AppError> {
    Ok(ProductJobNodeDefinition {
        id: "metric".into(),
        name: "执行真实指标聚合".into(),
        definition: json!({"format_version":1,"type":"metric_aggregation","metadata_id":snapshot.metadata.id,"metadata_version":snapshot.metadata.version,
            "publication_hash":super::freeze::hash(&snapshot.publication)?,"window":snapshot.window,"row_limit":snapshot.request.row_limit}),
        depends_on: vec![],
    })
}
pub(super) fn snapshot(job: &ProductJob) -> Result<MetricSnapshot, AppError> {
    let frozen: MetricSnapshot = serde_json::from_value(job.definition_snapshot.clone())
        .map_err(|_| bad("指标运行快照无效"))?;
    let publication = &frozen.publication;
    validate_idempotency_key(&frozen.request.idempotency_key)?;
    if !job.can_retry
        || job.action != "execute_metric"
        || !super::is_calculable(&job.source_kind)
        || frozen.format_version != 1
        || frozen.executor != "metric_aggregate_v1"
        || frozen.tenant_id != job.tenant_id
        || frozen.source_kind != job.source_kind
        || frozen.source_id != job.source_id
        || frozen.source_version != job.source_version
        || job.idempotency_key != frozen.request.idempotency_key
        || frozen.request.expected_version != job.source_version
        || job.parameters != json!(frozen.request)
        || frozen.properties["publication"] != json!(publication)
        || publication.format_version != 1
        || publication.executor != frozen.executor
        || publication.tenant_id != frozen.tenant_id
        || publication.source_kind != frozen.source_kind
        || publication.source_id != frozen.source_id
        || publication.source_version != frozen.source_version
        || publication.properties != super::definition::definition_properties(&frozen.properties)?
        || publication.metadata.as_ref() != Some(&frozen.metadata)
        || frozen.metadata.binding.engine != "swanlake"
        || frozen.metadata.binding.tenant_id != job.tenant_id
        || frozen.metadata.version != frozen.metadata.binding.object_version
        || !binding_matches(&frozen.metadata.binding, &frozen.metadata.properties)
        || frozen.metadata.properties["columns"] != json!(frozen.metadata.columns)
    {
        return Err(bad("指标实际来源、发布证明或执行快照不一致"));
    }
    let plan = publication
        .plan
        .as_ref()
        .ok_or_else(|| bad("指标冻结计划缺失"))?;
    if window::resolve(plan, &frozen.request)? != frozen.window {
        return Err(bad("指标数据窗与冻结请求不一致"));
    }
    Ok(frozen)
}
pub fn prepare_claimed(job: &ProductJob) -> Result<Vec<(String, String)>, AppError> {
    let frozen = snapshot(job)?;
    let expected = node(&frozen)?;
    if job.nodes.len() != 1
        || job.nodes[0].id != expected.id
        || job.nodes[0].name != expected.name
        || job.nodes[0].definition != expected.definition
        || job.nodes[0].depends_on != expected.depends_on
    {
        return Err(bad("指标执行节点与固定编译器不一致"));
    }
    Ok(vec![(expected.id, sql(&frozen)?)])
}
pub(crate) fn requires_full_table(job: &ProductJob) -> Result<bool, AppError> {
    let frozen = snapshot(job)?;
    Ok(frozen.publication.plan.as_ref().is_some_and(|plan| {
        plan.measures
            .iter()
            .any(|measure| measure.aggregate == "COUNT" && measure.argument.is_none())
    }))
}
