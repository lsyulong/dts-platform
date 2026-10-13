// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::services::query_svc::{ColumnMeta, QueryResult};
use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataEngineBinding, MetadataTable};

fn recognition_snapshot() -> RecognitionSnapshot {
    let columns = vec![MetadataColumn {
        name: "email".into(),
        data_type: "VARCHAR".into(),
        nullable: true,
        ordinal_position: 1,
    }];
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "customers".into(),
        table_type: "BASE TABLE".into(),
        columns: columns.clone(),
    };
    let class = FrozenResource {
        id: "class".into(),
        version: 2,
        properties: json!({"rank":2,"handling_policy":"需要人工确认后单独设置策略"}),
    };
    let rule = FrozenResource {
        id: "rule".into(),
        version: 2,
        properties: json!({"classification_id":"class","pattern":"^[^@]+@[^@]+[.]com$","match_target":"sample_value","sample_limit":10,"source_types":["text"],"publication":{"format_version":1,"classification":class}}),
    };
    let meta = RecognitionMetadata {
        id: "metadata".into(),
        version: 1,
        properties: table.properties("a").unwrap(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "a".into(),
            object_version: 1,
            table_catalog: table.table_catalog.clone(),
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            source_fingerprint: table.fingerprint().unwrap(),
        },
        columns,
    };
    let props = json!({"rule_id":"rule","object_id":"metadata","sample_limit":3,"publication":{"format_version":1,"rule":rule,"classification":class,"metadata":meta}});
    RecognitionSnapshot {
        format_version: 1,
        executor: "sensitive_scan_v1".into(),
        tenant_id: "a".into(),
        source_kind: "recognition_task".into(),
        source_id: "task".into(),
        source_version: 2,
        properties: props.clone(),
        task: FrozenResource {
            id: "task".into(),
            version: 2,
            properties: props,
        },
        rule,
        classification: class,
        metadata: meta,
    }
}
fn raw(rows: Vec<Value>) -> QueryResult {
    QueryResult {
        columns: vec![ColumnMeta {
            name: "email".into(),
            data_type: "Utf8".into(),
            is_numeric: false,
        }],
        row_count: rows.len(),
        rows,
        suggested_chart: None,
    }
}

#[test]
fn bounded_rust_regex_and_costs_reject_unsupported_rules() {
    for pattern in ["(?=secret)", r"(abc)\1", "[", &"x".repeat(1025)] {
        assert!(compiler::rule(&json!({"classification_id":"class","pattern":pattern})).is_err());
    }
    assert!(
        compiler::rule(&json!({"classification_id":"class","pattern":"a","sample_limit":1001}))
            .is_err()
    );
    assert!(
        compiler::rule(
            &json!({"classification_id":"class","pattern":"a","source_types":["text","text"]})
        )
        .is_err()
    );
    assert!(compiler::classification(&json!({"rank":0,"handling_policy":"protected"})).is_err());
}
#[test]
fn observed_sample_counts_nulls_and_never_persists_original_values() {
    let snapshot = recognition_snapshot();
    let result = matcher::evaluate(
        &snapshot,
        raw(vec![
            json!({"email":"person@company.com"}),
            json!({"email":"not an address"}),
            json!({"email":null}),
        ]),
    )
    .unwrap();
    assert_eq!(result.sampled_rows, 3);
    assert_eq!(result.evaluation_scope, "sample");
    assert_eq!(
        (
            result.columns[0].checked_values,
            result.columns[0].matched_values,
            result.columns[0].null_values
        ),
        (2, 1, 1)
    );
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("person@company.com"));
    assert!(!serialized.contains("not an address"));
    assert!(!result.producer_committed);
    assert!(result.finding_ids.is_empty());
    validate_result(&json!(snapshot), &result).unwrap();
}
#[test]
fn empty_sample_is_empty_observation_and_not_full_table_clearance() {
    let result = matcher::evaluate(&recognition_snapshot(), raw(vec![])).unwrap();
    assert_eq!(result.sampled_rows, 0);
    assert_eq!(result.columns[0].matched_values, 0);
    assert_eq!(result.evaluation_scope, "sample");
}
#[test]
fn limits_and_schema_drift_fail_instead_of_truncating() {
    let snapshot = recognition_snapshot();
    assert!(matcher::evaluate(&snapshot, raw(vec![json!({"email":"x"}); 4])).is_err());
    assert!(matcher::evaluate(&snapshot, raw(vec![json!({"email":"x".repeat(4097)})])).is_err());
    assert!(
        matcher::evaluate(&snapshot, raw(vec![json!({"email":"x","secret":"extra"})])).is_err()
    );
    let mut schema = raw(vec![]);
    schema.columns[0].data_type = "Int32".into();
    assert!(matcher::evaluate(&snapshot, schema).is_err());
    let mut rows = raw(vec![]);
    rows.row_count = 1;
    assert!(matcher::evaluate(&snapshot, rows).is_err());
}
#[test]
fn column_name_mode_never_reads_source_rows() {
    let mut snapshot = recognition_snapshot();
    snapshot.rule.properties["match_target"] = json!("column_name");
    snapshot.rule.properties["pattern"] = json!("^email$");
    snapshot.properties["publication"]["rule"] = json!(snapshot.rule);
    snapshot.task.properties = snapshot.properties.clone();
    assert!(compiler::sql(&snapshot).unwrap().ends_with("LIMIT 0"));
    let result = matcher::evaluate(&snapshot, raw(vec![])).unwrap();
    assert_eq!(result.evaluation_scope, "column_names");
    assert_eq!(result.columns[0].checked_values, 1);
    assert_eq!(result.columns[0].matched_values, 1);
    assert!(
        matcher::evaluate(&snapshot, raw(vec![json!({"email":"person@company.com"})])).is_err()
    );
}
#[test]
fn source_binding_and_sampling_budget_cannot_be_overridden() {
    let mut snapshot = recognition_snapshot();
    snapshot.metadata.binding.tenant_id = "b".into();
    assert!(compiler::sql(&snapshot).is_err());
    let mut snapshot = recognition_snapshot();
    snapshot.properties["sample_limit"] = json!(11);
    snapshot.task.properties = snapshot.properties.clone();
    assert!(compiler::sql(&snapshot).is_err());
    let mut snapshot = recognition_snapshot();
    snapshot.metadata.properties["table_name"] = json!("other");
    assert!(compiler::sql(&snapshot).is_err());
}
#[test]
fn producer_rejects_client_completion_markers_and_changed_counts() {
    let snapshot = recognition_snapshot();
    let mut result = matcher::evaluate(&snapshot, raw(vec![json!({"email":"x"})])).unwrap();
    result.producer_committed = true;
    assert!(validate_result(&json!(snapshot), &result).is_err());
    result.producer_committed = false;
    result.columns[0].matched_values = 2;
    assert!(validate_result(&json!(snapshot), &result).is_err());
    result.columns[0].matched_values = 0;
    result.sample_limit = 2;
    assert!(validate_result(&json!(snapshot), &result).is_err());
}
#[test]
fn typed_review_requires_reason_and_positive_expected_version() {
    let mut request = SensitiveReviewParams {
        tenant_id: "a".into(),
        actor_id: "owner".into(),
        finding_id: "finding".into(),
        expected_version: 1,
        confirm: true,
        reason: "实际业务人工复核".into(),
    };
    validate_review(&request).unwrap();
    request.reason = "  ".into();
    assert!(validate_review(&request).is_err());
    request.reason = "valid".into();
    request.expected_version = 0;
    assert!(validate_review(&request).is_err());
}
