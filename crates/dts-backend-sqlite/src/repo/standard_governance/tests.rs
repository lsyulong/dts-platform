use super::{dependencies, physical};
use dts_backend::repo::standard_governance::*;
use serde_json::json;

#[test]
fn duplicate_references_count_once_but_conflicting_hashes_are_rejected() {
    let reference = FrozenStandardRef {
        kind: "field_standard".into(),
        id: "field".into(),
        revision_version: 2,
        definition_hash: "a".repeat(64),
    };
    assert_eq!(
        dependencies::keys(&vec![reference.clone(); 1000])
            .unwrap()
            .len(),
        1
    );
    let mut other = reference.clone();
    other.definition_hash = "b".repeat(64);
    assert!(dependencies::keys(&[reference, other]).is_err());
}
#[test]
fn physical_type_aliases_match_without_merging_different_timestamp_or_decimal_semantics() {
    for (declared, actual) in [
        ("REAL", "FLOAT"),
        ("DATETIME", "TIMESTAMP"),
        ("TIMESTAMP WITH TIME ZONE", "TIMESTAMPTZ"),
        ("BYTEA", "BLOB"),
        ("NUMERIC(38, 8)", "DECIMAL(38,8)"),
        ("DOUBLE PRECISION", "DOUBLE"),
    ] {
        let model = json!({"table_name":"orders","columns":[{"name":"value","data_type":declared,"nullable":true}]});
        let mut receipt = json!({"generated_ddl":"CREATE TABLE orders","reconciled":true,"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":[{"column_name":"value","data_type":actual,"is_nullable":"YES"}]});
        assert!(
            physical::receipt_valid(&model, &receipt),
            "{declared} / {actual}"
        );
        receipt["physical_columns"][0]["data_type"] = json!("UNKNOWN");
        assert!(!physical::receipt_valid(&model, &receipt));
    }
    let model = json!({"table_name":"orders","columns":[{"name":"id","data_type":"TIMESTAMP"}]});
    let receipt = json!({"generated_ddl":"CREATE TABLE orders","reconciled":true,"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":[{"column_name":"id","data_type":"TIMESTAMPTZ","is_nullable":"YES"}]});
    assert!(!physical::receipt_valid(&model, &receipt));
}
#[test]
fn physical_primary_key_nullability_and_exact_table_identity_are_required() {
    let model = json!({"table_name":"orders","columns":[{"name":"id","data_type":"BIGINT","nullable":true}],"primary_key":["ID"]});
    let mut receipt = json!({"generated_ddl":"CREATE TABLE orders","reconciled":true,"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":[{"column_name":"id","data_type":"BIGINT","is_nullable":"NO"}]});
    assert!(physical::receipt_valid(&model, &receipt));
    receipt["physical_columns"][0]["is_nullable"] = json!("YES");
    assert!(!physical::receipt_valid(&model, &receipt));
    receipt["physical_columns"][0]["is_nullable"] = json!("NO");
    receipt["physical_binding"]["schema_name"] = json!("other");
    assert!(!physical::receipt_valid(&model, &receipt));
}
