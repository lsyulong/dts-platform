// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{bad, types::*};
use crate::models::error::AppError;
use dts_backend::repo::metadata_catalog::MetadataColumn;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn parse(
    properties: &Value,
    columns: &[MetadataColumn],
    title: &str,
) -> Result<ReportDefinition, AppError> {
    if properties
        .get("sql")
        .is_some_and(|value| !value.is_null() && value.as_str() != Some(""))
    {
        return Err(bad(
            "BI 报表采用真实对象的维度与度量定义，不能同时提供自由 SQL",
        ));
    }
    let dimensions: Vec<String> = serde_json::from_value(properties["dimensions"].clone())
        .map_err(|_| bad("维度必须是实际字段名称数组"))?;
    let measures: Vec<ReportMeasure> = serde_json::from_value(properties["measures"].clone())
        .map_err(|_| bad("度量须为名称、单字段表达式与聚合类型"))?;
    if dimensions.len() > 16 || measures.is_empty() || measures.len() > 32 {
        return Err(bad("BI 报表最多 16 个维度与 1 至 32 个度量"));
    }
    let row_limit = properties
        .get("row_limit")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_u64()
                .filter(|value| (1..=1000).contains(value))
                .ok_or_else(|| bad("报表行上限必须是 1 至 1000 的整数"))
        })
        .transpose()?
        .unwrap_or(500) as u32;
    let mut names = BTreeSet::new();
    for name in &dimensions {
        identifier(name)?;
        if !columns.iter().any(|column| &column.name == name)
            || !names.insert(name.to_ascii_lowercase())
        {
            return Err(bad("维度字段不存在于真实对象或重复"));
        }
    }
    for measure in &measures {
        identifier(&measure.name)?;
        if !names.insert(measure.name.to_ascii_lowercase())
            || columns
                .iter()
                .any(|column| column.name.eq_ignore_ascii_case(&measure.name))
        {
            return Err(bad(
                "度量结果别名须唯一且不能覆盖真实字段名，请使用 total_amount 等别名",
            ));
        }
        if measure.aggregate == Aggregate::Count && measure.expression == "*" {
            continue;
        }
        let column = columns
            .iter()
            .find(|column| column.name == measure.expression)
            .ok_or_else(|| bad("度量表达式必须是实际单个字段，COUNT 可使用 *"))?;
        identifier(&column.name)?;
        if matches!(measure.aggregate, Aggregate::Sum | Aggregate::Avg)
            && !numeric(&column.data_type)
        {
            return Err(bad("SUM/AVG 只能聚合真实数值字段"));
        }
    }
    let chart_config = properties
        .get("chart_config")
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|_| bad("图表配置不符合实际图表字段合同"))
        })
        .transpose()?
        .unwrap_or(ChartConfig {
            chart_type: ChartType::Table,
            title: title.into(),
            x_field: String::new(),
            y_field: String::new(),
            legend: true,
        });
    let report = ReportDefinition {
        dimensions,
        measures,
        row_limit,
        chart_config,
    };
    chart(&report, columns)?;
    Ok(report)
}

pub(super) fn identifier(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty()
        || name.len() > 256
        || name.chars().any(char::is_control)
        || name.contains(['"', '\\'])
    {
        return Err(bad("报表字段或别名为空、过长或含暂不支持的引号/反斜线"));
    }
    Ok(())
}

fn chart(report: &ReportDefinition, columns: &[MetadataColumn]) -> Result<(), AppError> {
    let chart = &report.chart_config;
    if chart.title.trim().is_empty()
        || chart.title.chars().count() > 120
        || chart.title.chars().any(char::is_control)
    {
        return Err(bad("图表标题必须为 1 至 120 个无控制字符的字符"));
    }
    if !chart.x_field.is_empty() && !report.dimensions.contains(&chart.x_field) {
        return Err(bad("图表 X 字段必须是实际输出维度"));
    }
    let measure = report
        .measures
        .iter()
        .find(|measure| measure.name == chart.y_field);
    if !chart.y_field.is_empty() && measure.is_none() {
        return Err(bad("图表 Y 字段必须是实际输出度量别名"));
    }
    if chart.chart_type != ChartType::Table {
        if report.dimensions.len() != 1 || chart.x_field.is_empty() {
            return Err(bad("当前图表需要一个真实分组维度；多维结果请使用表格"));
        }
        let measure = measure.ok_or_else(|| bad("图表需要实际度量 Y 字段"))?;
        if !matches!(
            measure.aggregate,
            Aggregate::Sum | Aggregate::Avg | Aggregate::Count
        ) && !columns
            .iter()
            .any(|column| column.name == measure.expression && numeric(&column.data_type))
        {
            return Err(bad(
                "图表 Y 字段必须是数值度量；文本或日期 MIN/MAX 请使用表格",
            ));
        }
    }
    Ok(())
}

fn numeric(data_type: &str) -> bool {
    matches!(
        data_type
            .split('(')
            .next()
            .unwrap_or(data_type)
            .trim()
            .to_ascii_uppercase()
            .as_str(),
        "TINYINT"
            | "SMALLINT"
            | "INTEGER"
            | "INT"
            | "BIGINT"
            | "HUGEINT"
            | "UTINYINT"
            | "USMALLINT"
            | "UINTEGER"
            | "UBIGINT"
            | "UHUGEINT"
            | "FLOAT"
            | "REAL"
            | "DOUBLE"
            | "DECIMAL"
            | "NUMERIC"
    )
}
