// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub node_id: String,
    pub expected_source_version: i64,
    pub format: ExportFormat,
}

#[derive(Debug, Clone, Serialize)]
pub struct CsvEncoding {
    pub version: u32,
    pub charset: &'static str,
    pub delimiter: &'static str,
    pub line_ending: &'static str,
    pub null_token: &'static str,
    pub backslash_escape: &'static str,
    pub spreadsheet_text_prefix: &'static str,
    pub decoding_order: &'static str,
}

pub struct ExportArtifact {
    pub(super) bytes: Vec<u8>,
    pub(super) filename: String,
    pub(super) content_type: &'static str,
    pub row_count: usize,
    pub source_version: i64,
    pub csv_encoding: Option<CsvEncoding>,
}
impl ExportArtifact {
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn content_type(&self) -> &'static str {
        self.content_type
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
