// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{ExportColumn, ExportData, ExportFormat, MAX_BYTES, encode, validate};
use serde_json::json;

fn data() -> ExportData {
    ExportData {
        source_kind: "bi_report".into(),
        source_id: "report".into(),
        source_version: 3,
        job_id: "job\r\nattachment:x".into(),
        node_id: "report".into(),
        columns: vec![
            ExportColumn {
                name: "label".into(),
                data_type: "Utf8".into(),
                is_numeric: false,
            },
            ExportColumn {
                name: "amount".into(),
                data_type: "Decimal128(18,2)".into(),
                is_numeric: true,
            },
        ],
        rows: vec![json!({"label":"华东","amount":"9007199254740991.25"})],
        row_count: 1,
    }
}

#[test]
fn json_keeps_exact_precision_null_and_unicode_without_private_definition_or_receipt() {
    let mut data = data();
    data.rows.push(json!({"label":null,"amount":"0.00"}));
    data.row_count = 2;
    let artifact = encode::artifact(&data, ExportFormat::Json).unwrap();
    let actual: serde_json::Value = serde_json::from_slice(artifact.as_bytes()).unwrap();
    assert_eq!(actual["rows"][0]["amount"], "9007199254740991.25");
    assert_eq!(actual["rows"][1]["label"], serde_json::Value::Null);
    assert_eq!(actual["row_count"], 2);
    assert!(actual.get("privacy").is_none());
    assert!(actual.get("definition_snapshot").is_none());
    assert!(!artifact.filename().contains(['\r', '\n', ':']));
}

#[test]
fn csv_blocks_formula_text_after_leading_space_control_or_unicode_whitespace() {
    for value in [
        "=SUM(1,2)",
        " +1",
        "-2+3",
        "@HYPERLINK(\"x\")",
        "\r\n\t=cmd",
        "\u{00a0}=cmd",
        "'already quoted",
        "9007199254740991.25",
    ] {
        assert!(
            encode::cell(&json!(value)).unwrap().starts_with('\''),
            "unsafe {value:?}"
        );
    }
    assert_eq!(encode::cell(&json!(-12)).unwrap(), "-12");
    assert_eq!(
        encode::cell(&json!(9007199254740991_i64)).unwrap(),
        "'9007199254740991"
    );
}

#[test]
fn csv_null_literal_token_and_backslash_text_remain_distinct_and_quotes_are_escaped() {
    assert_eq!(encode::cell(&json!(null)).unwrap(), "\\N");
    assert_eq!(encode::cell(&json!("\\N")).unwrap(), "\\\\N");
    assert_eq!(encode::cell(&json!("'\\N")).unwrap(), "''\\\\N");
    let mut data = data();
    data.rows[0]["label"] = json!("a,\"b\"\r\nc");
    let artifact = encode::artifact(&data, ExportFormat::Csv).unwrap();
    let actual = std::str::from_utf8(artifact.as_bytes()).unwrap();
    assert!(actual.starts_with("\"label\",\"amount\"\r\n"));
    assert!(actual.contains("\"a,\"\"b\"\"\r\nc\""));
    assert_eq!(artifact.csv_encoding.as_ref().unwrap().null_token, "\\N");
    assert!(encode::cell(&json!("bad\u{0000}text")).is_err());
}

#[test]
fn incomplete_rows_inconsistent_count_and_byte_overflow_never_become_partial_success() {
    let mut data = data();
    data.rows[0] = json!({"label":"missing amount"});
    assert!(validate(&data.columns, &data.rows, data.row_count).is_err());
    data = super::tests::data();
    data.row_count = 2;
    assert!(encode::artifact(&data, ExportFormat::Csv).is_err());
    data = super::tests::data();
    data.rows[0]["label"] = json!("a".repeat(MAX_BYTES));
    assert!(encode::artifact(&data, ExportFormat::Csv).is_err());
    assert!(encode::artifact(&data, ExportFormat::Json).is_err());
}
