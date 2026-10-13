// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{compiler, decode_result};
use crate::services::query_svc::{ColumnMeta, QueryResult};
use dts_backend::repo::metadata_catalog::MetadataScope;
use serde_json::{Value, json};

fn scope() -> MetadataScope {
    MetadataScope {
        engine: "swanlake".into(),
        table_catalog: None,
        schema_name: None,
        table_name: None,
    }
}
fn query_result(rows: Vec<Value>) -> QueryResult {
    QueryResult {
        columns: [
            "table_catalog",
            "schema_name",
            "table_name",
            "table_type",
            "column_name",
            "data_type",
            "is_nullable",
            "ordinal_position",
        ]
        .into_iter()
        .map(|name| ColumnMeta {
            name: name.into(),
            data_type: "fixture".into(),
            is_numeric: false,
        })
        .collect(),
        row_count: rows.len(),
        rows,
        suggested_chart: None,
    }
}
fn row(name: &str, ordinal: u32) -> Value {
    json!({"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE",
    "column_name":name,"data_type":"BIGINT","is_nullable":"NO","ordinal_position":ordinal})
}

#[test]
fn exact_scope_uses_safe_ast_literals_and_never_scans_external_locations() {
    let scope=compiler::scope(&json!({"source_type":"swanlake","schema_filter":"a'; UNION SELECT secret --","table_filter":"O''Reilly"})).unwrap();
    let sql = compiler::query(&scope).unwrap();
    assert!(sql.contains("information_schema.tables"));
    assert!(sql.contains("information_schema.columns"));
    assert!(sql.contains("'a''; UNION SELECT secret --'"));
    assert!(sql.contains("'O''''Reilly'"));
    assert!(!sql.contains('$'));
    assert!(!sql.contains(" LIMIT "));
    assert!(
        compiler::scope(&json!({"source_type":"datasource","datasource_id":"existing"})).is_err()
    );
    assert!(
        compiler::scope(&json!({"source_type":"swanlake","datasource_id":"arbitrary"})).is_err()
    );
    assert!(
        compiler::scope(&json!({"source_type":"swanlake","engine_binding":{"engine":"swanlake"}}))
            .is_err()
    );
    assert!(compiler::scope(&json!({"schema_filter":"main"})).is_err());
}

#[test]
fn decoder_groups_real_rows_and_sorts_actual_column_positions() {
    let decoded =
        decode_result(&scope(), query_result(vec![row("amount", 2), row("id", 1)])).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].columns[0].name, "id");
    assert_eq!(decoded[0].columns[1].ordinal_position, 2);
    assert!(!decoded[0].columns[0].nullable);
    let first = decoded[0].fingerprint().unwrap();
    let same = decode_result(&scope(), query_result(vec![row("id", 1), row("amount", 2)])).unwrap();
    assert_eq!(first, same[0].fingerprint().unwrap());
    assert_eq!(
        decoded[0].properties("tenant").unwrap()["engine_binding"],
        json!({"engine":"swanlake","tenant_id":"tenant"})
    );
}

#[test]
fn decoder_rejects_partial_shapes_duplicate_columns_invalid_scope_and_row_counts() {
    assert!(decode_result(&scope(), query_result(vec![row("id", 1), row("id", 2)])).is_err());
    assert!(decode_result(&scope(), query_result(vec![row("id", 1), row("amount", 1)])).is_err());
    let mut invalid = row("id", 1);
    invalid["is_nullable"] = json!("maybe");
    assert!(decode_result(&scope(), query_result(vec![invalid])).is_err());
    let limited = MetadataScope {
        schema_name: Some("other".into()),
        ..scope()
    };
    assert!(decode_result(&limited, query_result(vec![row("id", 1)])).is_err());
    let mut partial = query_result(vec![row("id", 1)]);
    partial.row_count = 2;
    assert!(decode_result(&scope(), partial).is_err());
    let mut missing = query_result(vec![]);
    missing.columns.pop();
    assert!(decode_result(&scope(), missing).is_err());
    assert!(
        decode_result(&scope(), query_result(vec![]))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn source_identity_separates_catalogs_and_structural_fingerprint_tracks_changes() {
    let first = decode_result(&scope(), query_result(vec![row("id", 1)]))
        .unwrap()
        .remove(0);
    let mut other = first.clone();
    other.table_catalog = "other_lake".into();
    assert_ne!(
        first.identity_code("a", "swanlake").unwrap(),
        other.identity_code("a", "swanlake").unwrap()
    );
    assert_ne!(
        first.identity_code("a", "swanlake").unwrap(),
        first.identity_code("b", "swanlake").unwrap()
    );
    other = first.clone();
    other.columns[0].nullable = true;
    assert_ne!(first.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(
        first.identity_code("a", "swanlake").unwrap(),
        other.identity_code("a", "swanlake").unwrap()
    );
}
