use super::bad;
use crate::models::error::AppError;
use dts_backend::repo::metadata_catalog::MetadataColumn;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn identifier(value: &str) -> bool {
    value.len() <= 128
        && !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        })
}
pub(super) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, AppError> {
    value[key]
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= 8192 && !value.contains('\0'))
        .ok_or_else(|| bad(format!("缺少有效{key}")))
}
pub(super) fn requested_columns(
    value: Option<&Value>,
    columns: &[MetadataColumn],
) -> Result<Vec<String>, AppError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(columns.iter().map(|column| column.name.clone()).collect());
    };
    let array = value
        .as_array()
        .filter(|array| array.len() <= 1000)
        .ok_or_else(|| bad("申请字段必须是最多1000项的字段名列表"))?;
    if array.is_empty() {
        return Ok(columns.iter().map(|column| column.name.clone()).collect());
    }
    let mut seen = HashSet::new();
    array
        .iter()
        .map(|value| {
            let name = value
                .as_str()
                .ok_or_else(|| bad("申请字段必须是真实字段名称"))?;
            let column = columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| bad("申请字段不存在于真实元数据中"))?;
            if !seen.insert(column.name.clone()) {
                return Err(bad("申请字段重复"));
            }
            Ok(column.name.clone())
        })
        .collect()
}
