// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::bad;
use crate::{models::error::AppError, services::query_svc::QueryResult};
use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataScope, MetadataTable};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn decode_result(
    scope: &MetadataScope,
    result: QueryResult,
) -> Result<Vec<MetadataTable>, AppError> {
    let required = [
        "table_catalog",
        "schema_name",
        "table_name",
        "table_type",
        "column_name",
        "data_type",
        "is_nullable",
        "ordinal_position",
    ];
    let names: BTreeSet<_> = result
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    if names.len() != required.len()
        || result.columns.len() != required.len()
        || required.iter().any(|name| !names.contains(name))
        || result.row_count != result.rows.len()
        || result.rows.len() > 10000
    {
        return Err(bad("元数据完整查询结果结构或容量无效，未应用快照"));
    }
    let mut tables: BTreeMap<(String, String, String), MetadataTable> = BTreeMap::new();
    let mut empty_tables = BTreeSet::new();
    for row in result.rows {
        let table = MetadataTable {
            table_catalog: text(&row, "table_catalog", 256)?,
            schema_name: text(&row, "schema_name", 256)?,
            table_name: text(&row, "table_name", 256)?,
            table_type: text(&row, "table_type", 32)?,
            columns: vec![],
        };
        if !scope.contains(&table) || !["BASE TABLE", "VIEW"].contains(&table.table_type.as_str()) {
            return Err(bad("查询返回的表超出采集范围或类型无效"));
        }
        let key = (
            table.table_catalog.clone(),
            table.schema_name.clone(),
            table.table_name.clone(),
        );
        let entry = tables.entry(key.clone()).or_insert(table.clone());
        if entry.table_type != table.table_type {
            return Err(bad("同一表返回不一致的类型"));
        }
        let column = row
            .get("column_name")
            .ok_or_else(|| bad("字段名称列缺失"))?;
        if column.is_null() {
            if !entry.columns.is_empty()
                || !empty_tables.insert(key.clone())
                || ["data_type", "is_nullable", "ordinal_position"]
                    .iter()
                    .any(|key| row.get(*key) != Some(&Value::Null))
            {
                return Err(bad("空字段结构不是完整的 LEFT JOIN 结果"));
            }
            continue;
        }
        if empty_tables.contains(&key) {
            return Err(bad("同一表同时返回空字段与真实字段"));
        }
        let ordinal = row
            .get("ordinal_position")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| bad("字段顺序位置必须为正整数"))?;
        let nullable = match text(&row, "is_nullable", 8)?.as_str() {
            "YES" => true,
            "NO" => false,
            _ => return Err(bad("字段可空性不是 YES/NO")),
        };
        let column = MetadataColumn {
            name: text(&row, "column_name", 256)?,
            data_type: text(&row, "data_type", 512)?,
            nullable,
            ordinal_position: ordinal,
        };
        if entry
            .columns
            .iter()
            .any(|old| old.name == column.name || old.ordinal_position == ordinal)
        {
            return Err(bad("字段名称或顺序位置重复"));
        }
        entry.columns.push(column);
    }
    if tables.len() > 1000 {
        return Err(bad("完整采集超过 1000 个表，请缩小采集范围"));
    }
    Ok(tables
        .into_values()
        .map(|mut table| {
            table.columns.sort_by_key(|column| column.ordinal_position);
            table
        })
        .collect())
}

fn text(row: &Value, key: &str, max: usize) -> Result<String, AppError> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(|| bad("元数据标识或类型字段不是有效文本"))
}
