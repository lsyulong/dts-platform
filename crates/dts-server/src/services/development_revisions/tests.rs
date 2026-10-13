use super::*;
use chrono::Utc;
use serde_json::json;
fn resource(kind: &str, properties: serde_json::Value) -> ProductResource {
    ProductResource {
        id: "source".into(),
        tenant_id: "tenant".into(),
        kind: kind.into(),
        code: "source".into(),
        name: "Source".into(),
        description: None,
        parent_id: None,
        status: "draft".into(),
        properties,
        version: 1,
        created_by: "actor".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        references: vec![],
    }
}
#[test]
fn restore_validation_rejects_write_sql_wrong_parameters_and_fabricated_references() {
    assert!(
        validated(&resource(
            "sql_document",
            json!({"sql":"DELETE FROM secret","parameters":[]})
        ))
        .is_err()
    );
    assert!(validated(&resource("sql_document",json!({"sql":"SELECT $id","parameters":[{"name":"id","data_type":"PASSWORD","required":true}]}))).is_err());
    assert!(
        validated(&resource(
            "sql_document",
            json!({"sql":"SELECT 1","model_id":"missing"})
        ))
        .is_err()
    );
    assert!(validated(&resource("sql_document",json!({"sql":"SELECT $id","parameters":[{"name":"id","data_type":"BIGINT","required":true}]}))).is_ok());
    assert!(validated(&resource("development_workflow",json!({"definition":{"nodes":[{"id":"a","name":"A","sql":"SELECT 1","depends_on":["b"]},{"id":"b","name":"B","sql":"SELECT 2","depends_on":["a"]}]},"cron_expression":"0 0 * * * *","timezone":"UTC"}))).is_err());
}
#[test]
fn http_dtos_never_accept_client_snapshots_or_parameter_overrides() {
    let base = json!({"expected_version":1,"reason":"保存","idempotency_key":uuid::Uuid::new_v4().to_string()});
    assert!(serde_json::from_value::<SaveDevelopmentRevisionRequest>(base.clone()).is_ok());
    let mut forged = base.clone();
    forged["snapshot"] = json!({"sql":"SELECT 1"});
    assert!(serde_json::from_value::<SaveDevelopmentRevisionRequest>(forged).is_err());
    let mut forged = base;
    forged["source_version"] = json!(99);
    assert!(serde_json::from_value::<RestoreDevelopmentRevisionRequest>(forged).is_err());
}
#[test]
fn diff_returns_actual_paths_versions_and_bounded_truncation() {
    let before = resource("sql_document", json!({"sql":"SELECT 1","parameters":[]}));
    let summary = DevelopmentRevisionSummary {
        id: "rev".into(),
        tenant_id: "tenant".into(),
        source_kind: before.kind.clone(),
        source_id: before.id.clone(),
        source_version: 1,
        snapshot_hash: dts_backend::repo::development_revisions::validation::snapshot_hash(&before)
            .unwrap(),
        saved_by: "actor".into(),
        saved_at: Utc::now(),
        reason: "保存".into(),
    };
    let mut after = before.clone();
    after.version = 2;
    after.properties["sql"] = json!("SELECT 2");
    let result = diff::compare(
        DevelopmentRevision {
            summary: summary.clone(),
            snapshot: before.clone(),
        },
        after,
        None,
    )
    .unwrap();
    assert_eq!(result.to_version, 2);
    assert_eq!(result.changes[0].path, "/properties/sql");
    assert!(!result.truncated);
    let mut after = before.clone();
    after.properties = json!(
        (0..300)
            .map(|v| (format!("field{v}"), json!(v)))
            .collect::<serde_json::Map<_, _>>()
    );
    let result = diff::compare(
        DevelopmentRevision {
            summary,
            snapshot: before,
        },
        after,
        None,
    )
    .unwrap();
    assert_eq!(result.changes.len(), 256);
    assert!(result.truncated);
}
