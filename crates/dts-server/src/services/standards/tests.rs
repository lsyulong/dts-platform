use super::{
    bindings::project_binding_properties, gate, inspection::evaluate_snapshot, validation,
};
use chrono::Utc;
use dts_backend::{
    ProductResource,
    repo::{metadata_catalog::MetadataEngineBinding, standard_governance::*},
};
use serde_json::{Value, json};

fn resource(kind: &str, id: &str, properties: Value) -> ProductResource {
    ProductResource {
        id: id.into(),
        tenant_id: "tenant".into(),
        kind: kind.into(),
        code: id.into(),
        name: id.into(),
        description: None,
        parent_id: None,
        status: "published".into(),
        properties,
        version: 3,
        created_by: "actor".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        references: vec![],
    }
}
fn revision(kind: &str, properties: Value) -> StandardRevision {
    StandardRevision {
        tenant_id: "tenant".into(),
        kind: kind.into(),
        id: "std".into(),
        revision_version: 2,
        code: "code".into(),
        name: "standard".into(),
        definition_hash: revision_hash(kind, "code", "standard", &properties, &[]).unwrap(),
        properties,
        children: vec![],
        state: "active".into(),
        state_version: 1,
        published_by: "actor".into(),
        published_at: Utc::now(),
        withdrawn_by: None,
        withdrawn_at: None,
        withdrawal_reason: None,
    }
}
fn model() -> Value {
    json!({"layer_id":"layer","table_name":"main.orders","columns":[{"name":"id","data_type":"BIGINT","nullable":true}],"primary_key":["id"]})
}
#[test]
fn published_children_must_have_exact_positive_versions() {
    assert!(validation::normalized_standard("field_standard",&json!({"data_type":"VARCHAR","nullable":true,"definition":"真实业务定义","enum_id":"enum"})).is_err());
    assert!(validation::normalized_standard("field_standard",&json!({"data_type":"VARCHAR","definition":"真实业务定义","enum_id":"enum","enum_version":0})).is_err());
    let value = validation::normalized_standard(
        "field_standard",
        &json!({"data_type":"text","definition":"真实业务定义","enum_id":"enum","enum_version":2}),
    )
    .unwrap();
    assert_eq!(value["data_type"], "VARCHAR");
    assert_eq!(value["nullable"], true);
}
#[test]
fn draft_and_injected_evidence_cannot_be_revision_proof() {
    let mut standard = revision(
        "unit_standard",
        json!({"symbol":"kg","dimension":"mass","conversion_factor":1}),
    );
    assert!(validation::verify_revision(&standard, "otherTenant").is_err());
    standard.properties["conversion_factor"] = json!(1000);
    assert!(validation::verify_revision(&standard, "tenant").is_err());
    assert!(
        validation::normalized_standard(
            "unit_standard",
            &json!({"symbol":"kg","dimension":"mass","publication":{"trusted":true}})
        )
        .is_err()
    );
}
#[test]
fn bounded_full_name_match_and_object_type_are_enforced() {
    let standard = revision(
        "naming_standard",
        json!({"object_type":"column","pattern":"[a-z][a-z0-9_]*"}),
    );
    assert!(validation::check_name(&standard, "column", "order_id").is_ok());
    assert!(validation::check_name(&standard, "column", "prefix:order_id").is_err());
    assert!(validation::check_name(&standard, "table", "orders").is_err());
    assert!(
        validation::normalized_standard(
            "naming_standard",
            &json!({"object_type":"column","pattern":"(a)\\1"})
        )
        .is_err()
    );
}
#[test]
fn field_type_and_effective_primary_key_nullability_are_checked() {
    let properties = model();
    let standard = revision(
        "field_standard",
        json!({"data_type":"BIGINT","nullable":false,"definition":"业务主键"}),
    );
    assert!(validation::check_field(&properties, &properties["columns"][0], &standard).is_ok());
    let mut nullable = properties.clone();
    nullable["primary_key"] = json!([]);
    assert!(validation::check_field(&nullable, &nullable["columns"][0], &standard).is_err());
    let decimal = revision(
        "field_standard",
        json!({"data_type":"DECIMAL(20,2)","nullable":true,"definition":"金额"}),
    );
    assert!(validation::check_field(&properties, &properties["columns"][0], &decimal).is_err());
}
#[test]
fn binding_projection_uses_real_ids_and_versions_without_hash_injection() {
    let mut properties = model();
    let standard = revision(
        "field_standard",
        json!({"data_type":"BIGINT","nullable":false,"definition":"业务主键"}),
    );
    project_binding_properties(
        &mut properties,
        StandardCoverage::AllFields,
        &[ModelColumnStandardBinding {
            column_name: "id".into(),
            field_standard: Some(standard.reference()),
            naming_standard: None,
        }],
        &None,
    )
    .unwrap();
    assert_eq!(properties["columns"][0]["standard_id"], "std");
    assert_eq!(properties["columns"][0]["standard_version"], 2);
    assert!(properties["columns"][0].get("definition_hash").is_none());
    assert!(validation::model_properties(&properties).is_ok());
    properties["columns"][0]["trusted"] = json!(true);
    assert!(validation::model_properties(&properties).is_err());
}
#[test]
fn unconfigured_is_distinct_and_physical_receipt_cannot_approve_false_nullable() {
    let properties = model();
    assert!(!gate::has_declared_bindings(&properties));
    let receipt = json!({"generated_ddl":"CREATE TABLE","reconciled":true,"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":[{"column_name":"id","data_type":"BIGINT","is_nullable":"NO"}]});
    assert!(gate::verify_physical_receipt(&properties, &receipt).is_ok());
    let mut bad = receipt.clone();
    bad["physical_columns"][0]["is_nullable"] = json!("YES");
    assert!(gate::verify_physical_receipt(&properties, &bad).is_err());
    let mut wrong = receipt.clone();
    wrong["physical_binding"]["schema_name"] = json!("other");
    assert!(gate::verify_physical_receipt(&properties, &wrong).is_err());
}
fn inspection_fixture() -> StandardInspectionSnapshot {
    let standard = revision(
        "field_standard",
        json!({"data_type":"BIGINT","nullable":false,"definition":"业务主键"}),
    );
    let mut properties = model();
    let columns = vec![ModelColumnStandardBinding {
        column_name: "id".into(),
        field_standard: Some(standard.reference()),
        naming_standard: None,
    }];
    project_binding_properties(
        &mut properties,
        StandardCoverage::AllFields,
        &columns,
        &None,
    )
    .unwrap();
    let properties_hash = standard_hash(&properties).unwrap();
    StandardInspectionSnapshot {
        resource: resource("data_model", "model", properties),
        bindings: ModelStandardBindingSet {
            tenant_id: "tenant".into(),
            model_id: "model".into(),
            model_version: 1,
            binding_version: 1,
            coverage: StandardCoverage::AllFields,
            definition_hash: binding_hash(
                StandardCoverage::AllFields,
                &columns,
                &None,
                &properties_hash,
            )
            .unwrap(),
            columns,
            table_naming: None,
            model_properties_hash: properties_hash,
            created_by: "actor".into(),
            created_at: Utc::now(),
        },
        revisions: vec![standard],
        metadata: resource(
            "metadata_object",
            "metadata",
            json!({"columns":[{"name":"id","data_type":"BIGINT","nullable":false,"ordinal_position":1}]}),
        ),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant".into(),
            object_version: 3,
            table_catalog: "lake".into(),
            schema_name: "main".into(),
            table_name: "orders".into(),
            source_fingerprint: "actual-fingerprint".into(),
        },
        metadata_observed_at: Utc::now(),
        publication: resource(
            "model_publication",
            "receipt",
            json!({"receipt":{"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders"}}}),
        ),
        snapshot_hash: "sourcehash".into(),
    }
}
#[test]
fn collected_schema_is_explicitly_not_an_enum_value_or_unit_semantics_scan() {
    let snapshot = inspection_fixture();
    let result = evaluate_snapshot(&snapshot, vec![snapshot.revisions[0].reference()]).unwrap();
    assert_eq!(result.state, "conformant");
    assert_eq!(result.evaluation_scope, "collected_schema");
    assert!(!result.data_values_evaluated);
    assert!(!result.unit_semantics_evaluated);
    assert_eq!(result.source_fingerprint, "actual-fingerprint");
}
#[test]
fn structure_drift_is_nonconformant_and_other_catalog_is_not_same_table() {
    let mut snapshot = inspection_fixture();
    snapshot.metadata.properties["columns"][0]["nullable"] = json!(true);
    let result = evaluate_snapshot(&snapshot, vec![snapshot.revisions[0].reference()]).unwrap();
    assert_eq!(result.state, "non_conformant");
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == "standard_nullability_mismatch")
    );
    snapshot.binding.table_catalog = "another_lake".into();
    assert!(evaluate_snapshot(&snapshot, vec![]).is_err());
}
