// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{ExportData, MAX_BYTES, bad, types::*};
use crate::models::error::AppError;
use serde_json::{Value, json};

pub(super) fn artifact(
    data: &ExportData,
    format: ExportFormat,
) -> Result<ExportArtifact, AppError> {
    super::validate(&data.columns, &data.rows, data.row_count)?;
    let (bytes, content_type, csv_encoding, extension) = match format {
        ExportFormat::Json => (
            json_bytes(data)?,
            "application/json; charset=utf-8",
            None,
            "json",
        ),
        ExportFormat::Csv => (
            csv_bytes(data)?,
            "text/csv; charset=utf-8",
            Some(csv_contract()),
            "csv",
        ),
    };
    if bytes.len() > MAX_BYTES {
        return Err(bad("导出超过2MiB上限，未截断或生成部分成功文件"));
    }
    let id: String = data
        .job_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    Ok(ExportArtifact {
        bytes,
        content_type,
        filename: format!(
            "{}-{}.{}",
            data.source_kind,
            if id.is_empty() { "result" } else { &id },
            extension
        ),
        row_count: data.row_count,
        source_version: data.source_version,
        csv_encoding,
    })
}

fn json_bytes(data: &ExportData) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(&json!({"format_version":1,"source_kind":data.source_kind,"source_id":data.source_id,"source_version":data.source_version,
        "job_id":data.job_id,"node_id":data.node_id,"columns":data.columns,"rows":data.rows,"row_count":data.row_count,
        "output_scope":"current_reader_authorized_and_current_policy_applied","complete":true}))
        .map_err(|_|bad("真实JSON结果无法编码"))
}

fn csv_contract() -> CsvEncoding {
    CsvEncoding {
        version: 1,
        charset: "UTF-8",
        delimiter: ",",
        line_ending: "CRLF",
        null_token: "\\N",
        backslash_escape: "\\\\",
        spreadsheet_text_prefix: "'",
        decoding_order: "CSV parse; detect exact \\N as NULL; remove one leading apostrophe; decode doubled backslashes; apply declared column type",
    }
}

fn csv_bytes(data: &ExportData) -> Result<Vec<u8>, AppError> {
    let mut output = String::new();
    write_row(
        &mut output,
        data.columns
            .iter()
            .map(|column| text(&column.name))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    for row in &data.rows {
        let cells = data
            .columns
            .iter()
            .map(|column| cell(&row[&column.name]))
            .collect::<Result<Vec<_>, _>>()?;
        write_row(&mut output, cells)?;
    }
    Ok(output.into_bytes())
}

fn write_row(output: &mut String, cells: Vec<String>) -> Result<(), AppError> {
    for (index, cell) in cells.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push('"');
        for ch in cell.chars() {
            if ch == '"' {
                output.push('"');
            }
            output.push(ch);
        }
        output.push('"');
        if output.len() > MAX_BYTES {
            return Err(bad("CSV导出超过2MiB，未截断返回"));
        }
    }
    output.push_str("\r\n");
    if output.len() > MAX_BYTES {
        return Err(bad("CSV导出超过2MiB，未截断返回"));
    }
    Ok(())
}

pub(super) fn cell(value: &Value) -> Result<String, AppError> {
    match value {
        Value::Null => Ok("\\N".into()),
        Value::String(value) => text(value),
        Value::Number(value) => {
            let value = value.to_string();
            // Real numbers cannot inject a formula; lengthy representations
            // remain text to prevent spreadsheet precision loss.
            Ok(if value.bytes().filter(u8::is_ascii_digit).count() > 15 {
                format!("'{value}")
            } else {
                value
            })
        },
        Value::Bool(value) => Ok(value.to_string()),
        Value::Array(_) | Value::Object(_) => {
            text(&serde_json::to_string(value).map_err(|_| bad("复合真实值无法编码"))?)
        },
    }
}

fn text(value: &str) -> Result<String, AppError> {
    if value
        .chars()
        .any(|c| c.is_control() && !['\r', '\n', '\t'].contains(&c))
    {
        return Err(bad("CSV包含不支持的控制字符，请选择无损JSON导出"));
    }
    let first = value
        .trim_start_matches(|c: char| c.is_whitespace() || c.is_control())
        .chars()
        .next();
    let protect = first.is_some_and(|c| ['=', '+', '-', '@'].contains(&c))
        || value.starts_with('\'')
        || value.chars().next().is_some_and(char::is_control)
        || value.trim().parse::<f64>().is_ok();
    let encoded = value.replace('\\', "\\\\");
    Ok(if protect {
        format!("'{encoded}")
    } else {
        encoded
    })
}
