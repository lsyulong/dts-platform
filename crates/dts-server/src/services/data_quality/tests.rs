#![allow(clippy::unwrap_used)]

use super::*;
use crate::services::query_svc::QueryResult;
use chrono::Utc;

fn snapshot() -> QualitySnapshot {
    let resource = |id: &str, name: &str, properties: Value| FrozenResource {
        id: id.into(),
        version: 1,
        name: name.into(),
        properties,
    };
    let task_properties = json!({"object_id":"object-1","rule_ids":["rule-1"]});
    QualitySnapshot {
        format_version: 1,
        executor: "quality_count_v1".into(),
        tenant_id: "tenant-a".into(),
        source_kind: "quality_task".into(),
        source_id: "task-1".into(),
        source_version: 1,
        properties: task_properties.clone(),
        task: resource("task-1", "Quality", task_properties),
        object: resource(
            "object-1",
            "Orders",
            json!({"metadata_object_id":"metadata-1","columns":["id","amount","status"]}),
        ),
        metadata: resource(
            "metadata-1",
            "Fixture",
            json!({"schema_name":"main","table_name":"fixture","engine_binding":{"engine":"swanlake","tenant_id":"tenant-a"},"columns":[
            {"name":"id","data_type":"BIGINT","nullable":true},{"name":"amount","data_type":"DOUBLE","nullable":true},{"name":"status","data_type":"VARCHAR","nullable":false}]}),
        ),
        rules: vec![resource(
            "rule-1",
            "Amounts present",
            json!({"object_id":"object-1","rule_type":"not_null","column_name":"amount","conditions":[]}),
        )],
    }
}

fn definition(rule_type: &str, target: &str, conditions: Value) -> Value {
    let mut frozen = snapshot();
    frozen.rules[0].properties = json!({"object_id":"object-1","rule_type":rule_type,"column_name":target,"conditions":conditions});
    snapshot::nodes(&frozen).unwrap().remove(0).definition
}

fn counts(checked: Value, failed: Value) -> QueryResult {
    QueryResult {
        columns: vec![],
        rows: vec![json!({"checked_rows":checked,"failed_rows":failed})],
        row_count: 1,
        suggested_chart: None,
    }
}

fn job(result: Option<Value>) -> ProductJob {
    let frozen = snapshot();
    let node = snapshot::nodes(&frozen).unwrap().remove(0);
    let now = Utc::now();
    ProductJob {
        id: "job-1".into(),
        tenant_id: "tenant-a".into(),
        source_kind: "quality_task".into(),
        source_id: "task-1".into(),
        source_version: 1,
        actor_id: "actor-1".into(),
        action: "execute_quality".into(),
        idempotency_key: "request-1".into(),
        definition_snapshot: serde_json::to_value(frozen).unwrap(),
        parameters: json!({}),
        status: ProductJobStatus::Succeeded,
        cancel_requested: false,
        cancel_requested_by: None,
        can_retry: true,
        attempt: 1,
        lease_owner: None,
        lease_fence: 1,
        lease_expires_at: None,
        result: None,
        error: None,
        reconciled_by: None,
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: Some(now),
        nodes: vec![ProductJobNode {
            id: node.id,
            name: node.name,
            definition: node.definition,
            depends_on: node.depends_on,
            status: ProductJobNodeStatus::Succeeded,
            attempt: 1,
            result,
            error: None,
            started_at: Some(now),
            finished_at: Some(now),
        }],
    }
}

#[test]
fn compilation_is_real_read_only_count_and_nulls_fail() {
    let sql = compile_node(&definition("not_null", "amount", json!([]))).unwrap();
    assert!(sql.contains("COUNT(*) AS checked_rows"));
    assert!(sql.contains("\"amount\" IS NOT NULL"));
    assert!(sql.contains("COALESCE"));
    let unique = compile_node(&definition("unique", "id", json!([]))).unwrap();
    assert!(unique.contains("COUNT(*) OVER(PARTITION BY \"id\")"));
    assert!(unique.contains("IS NULL OR"));
}

#[test]
fn dangerous_unknown_and_ignored_rules_are_rejected() {
    for conditions in [
        json!({"expression":"TRUE"}),
        json!({"expression":"SUM(amount)>0"}),
        json!({"expression":"amount>(SELECT 1)"}),
        json!({"expression":"amount>0; DROP TABLE fixture"}),
        json!({"expression":"amount>getenv('TOKEN')"}),
        json!({"expression":"unknown>0"}),
        json!({"expression":"status='active'"}),
        json!({"expression":"amount>0 OVER ()"}),
        json!({"expression":"amount + 1"}),
        json!({"expression":"amount AND amount > 0"}),
    ] {
        let mut node = definition("not_null", "amount", json!([]));
        node["rule_type"] = json!("expression");
        node["conditions"] = conditions;
        assert!(compile_node(&node).is_err());
    }
    let mut ignored = definition("not_null", "amount", json!([]));
    ignored["conditions"] = json!([{"column":"amount","operator":"gte","value":0}]);
    assert!(compile_node(&ignored).is_err());
    let mut injected = definition("not_null", "amount", json!([]));
    injected["table_name"] = json!("main.fixture;DROP");
    assert!(compile_node(&injected).is_err());
}

#[test]
fn numeric_range_is_typed_ordered_and_inclusive() {
    assert!(compile_node(&definition("range","amount",json!([{"column":"amount","operator":"gte","value":0},{"column":"amount","operator":"lte","value":0}]))).is_ok());
    for value in [json!("0"), json!(false), Value::Null] {
        let mut node = definition("not_null", "amount", json!([]));
        node["rule_type"] = json!("range");
        node["conditions"] = json!([{"column":"amount","operator":"gte","value":value}]);
        assert!(compile_node(&node).is_err());
    }
    let mut node = definition("not_null", "amount", json!([]));
    node["rule_type"] = json!("range");
    node["conditions"] = json!([{"column":"amount","operator":"gt","value":2},{"column":"amount","operator":"lte","value":2}]);
    assert!(compile_node(&node).is_err());
    node["conditions"] = json!([{"column":"amount","operator":"gte","value":3},{"column":"amount","operator":"lte","value":2}]);
    assert!(compile_node(&node).is_err());
}

#[test]
fn count_mapping_preserves_zero_and_rejects_non_counts() {
    let definition = definition("not_null", "amount", json!([]));
    let empty = map_node_result(&definition, counts(json!(0), json!(0))).unwrap();
    assert_eq!(empty["status"], "empty_data");
    assert_eq!(empty["failure_ratio"], Value::Null);
    let failed = map_node_result(&definition, counts(json!(100), json!(3))).unwrap();
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["failure_ratio"], 0.03);
    assert!(map_node_result(&definition, counts(json!(3), json!(4))).is_err());
    assert!(map_node_result(&definition, counts(json!(-1), json!(0))).is_err());
    assert!(map_node_result(&definition, counts(json!(1.5), json!(0))).is_err());
    assert!(map_node_result(&definition, counts(Value::Null, json!(0))).is_err());
    assert_eq!(
        map_node_result(&definition, counts(json!("9007199254740993"), json!("0"))).unwrap()["checked_rows"],
        9007199254740993u64
    );
}

#[test]
fn sampling_is_real_sql_limit_and_never_full_table_claim() {
    let mut node = definition("not_null", "amount", json!([]));
    node["sample_limit"] = json!(2);
    assert!(compile_node(&node).unwrap().contains("LIMIT 2"));
    assert_eq!(
        map_node_result(&node, counts(json!(2), json!(0))).unwrap()["evaluation_scope"],
        "sample"
    );
    assert!(map_node_result(&node, counts(json!(3), json!(0))).is_err());
}

#[test]
fn snapshots_bind_engine_real_columns_references_and_versions() {
    assert!(snapshot::validate_snapshot(&snapshot(), true).is_ok());
    let mut wrong = snapshot();
    wrong.metadata.properties["engine_binding"]["tenant_id"] = json!("tenant-b");
    assert!(snapshot::validate_snapshot(&wrong, true).is_err());
    assert!(snapshot::validate_snapshot(&wrong, false).is_ok());
    let mut unknown = snapshot();
    unknown.object.properties["columns"] = json!(["ghost"]);
    assert!(snapshot::validate_snapshot(&unknown, true).is_err());
    let mut other = snapshot();
    other.rules[0].properties["object_id"] = json!("object-2");
    assert!(snapshot::validate_snapshot(&other, true).is_err());
    let mut schedule = snapshot();
    schedule.properties["schedule"] = json!("0 * * * *");
    schedule.task.properties = schedule.properties.clone();
    assert!(snapshot::validate_snapshot(&schedule, true).is_err());
}

#[test]
fn physical_identity_includes_actual_catalog_and_rejects_invalid_namespace() {
    let mut frozen = snapshot();
    frozen.metadata.properties["table_catalog"] = json!("warehouse");
    let node = snapshot::nodes(&frozen).unwrap().remove(0).definition;
    assert_eq!(node["table_name"], "warehouse.main.fixture");
    assert!(
        compile_node(&node)
            .unwrap()
            .contains("\"warehouse\".\"main\".\"fixture\"")
    );
    frozen.metadata.properties["table_catalog"] = json!("bad;ATTACH");
    assert!(snapshot::nodes(&frozen).is_err());
}

#[test]
fn worker_hook_recompiles_and_rejects_tampered_nodes_or_identity() {
    let mut original = job(None);
    assert_eq!(prepare_claimed(&original).unwrap().len(), 1);
    original.nodes[0].definition["column_name"] = json!("id");
    assert!(prepare_claimed(&original).is_err());
    let mut actor_scope = job(None);
    actor_scope.tenant_id = "tenant-b".into();
    assert!(prepare_claimed(&actor_scope).is_err());
    let mut write = job(None);
    write.can_retry = false;
    assert!(prepare_claimed(&write).is_err());
}

#[test]
fn reports_use_only_real_results_and_never_pass_empty_or_missing_windows() {
    let node = definition("not_null", "amount", json!([]));
    let result = map_node_result(&node, counts(json!(0), json!(0))).unwrap();
    let original = job(Some(result));
    let start = original.finished_at.unwrap() - chrono::Duration::hours(1);
    let end = original.finished_at.unwrap() + chrono::Duration::hours(1);
    assert_eq!(
        report_from_jobs(
            "tenant-a",
            &[original.clone()],
            1,
            None,
            None,
            start,
            end,
            None
        )
        .unwrap()
        .status,
        "empty_data"
    );
    assert_eq!(
        report_from_jobs("tenant-a", &[], 0, None, None, start, end, None)
            .unwrap()
            .status,
        "no_results"
    );
    assert_eq!(
        report_from_jobs("tenant-a", &[original], 2, None, None, start, end, None)
            .unwrap()
            .status,
        "incomplete_window"
    );
}

#[test]
fn reports_validate_identity_and_error_partial_runs_without_invented_counts() {
    let mut original = job(None);
    original.status = ProductJobStatus::Failed;
    original.nodes[0].status = ProductJobNodeStatus::Failed;
    original.nodes[0].error = Some("实际查询错误".into());
    let start = original.finished_at.unwrap() - chrono::Duration::hours(1);
    let end = original.finished_at.unwrap() + chrono::Duration::hours(1);
    let report = report_from_jobs(
        "tenant-a",
        &[original.clone()],
        1,
        None,
        None,
        start,
        end,
        None,
    )
    .unwrap();
    assert_eq!(report.status, "execution_error");
    assert_eq!(report.checked_rows, 0);
    assert_eq!(report.failure_ratio, None);
    let results = results_from_jobs("tenant-a", &[original.clone()], 1).unwrap();
    assert_eq!(results.data[0].checked_rows, None);
    assert_eq!(results.data[0].error.as_deref(), Some("实际查询错误"));
    assert!(results_from_jobs("tenant-b", &[original], 1).is_err());
    let malformed = job(Some(json!({"checked_rows":0})));
    assert!(results_from_jobs("tenant-a", &[malformed], 1).is_err());
}

#[test]
fn monitor_threshold_does_not_erase_rule_failures_or_evaluate_missing_results() {
    let node = definition("not_null", "amount", json!([]));
    let original = job(Some(
        map_node_result(&node, counts(json!(100), json!(3))).unwrap(),
    ));
    let start = original.finished_at.unwrap() - chrono::Duration::hours(1);
    let end = original.finished_at.unwrap() + chrono::Duration::hours(1);
    let report = report_from_jobs(
        "tenant-a",
        &[original.clone()],
        1,
        None,
        None,
        start,
        end,
        Some(0.05),
    )
    .unwrap();
    assert_eq!(report.status, "within_threshold");
    assert_eq!(report.failed_rows, 3);
    assert_eq!(report.threshold_exceeded, Some(false));
    let exceeded = report_from_jobs(
        "tenant-a",
        &[original],
        1,
        None,
        None,
        start,
        end,
        Some(0.01),
    )
    .unwrap();
    assert_eq!(exceeded.status, "threshold_exceeded");
    assert_eq!(exceeded.threshold_exceeded, Some(true));
    let none = report_from_jobs("tenant-a", &[], 0, None, None, start, end, Some(0.1)).unwrap();
    assert_eq!(none.threshold_exceeded, None);
}

#[cfg(feature = "sqlite")]
mod sqlite;
