// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{compiler, definition, result::map_node_result, types::*};
use crate::services::query_svc::{ColumnMeta, QueryResult};
use chrono::Utc;
use dts_backend::repo::{
    metadata_catalog::{MetadataColumn, MetadataEngineBinding, MetadataTable},
    product_job::*,
};
use serde_json::{Value, json};

fn snapshot(properties: Value) -> ReportSnapshot {
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![
            MetadataColumn {
                name: "region".into(),
                data_type: "VARCHAR".into(),
                nullable: false,
                ordinal_position: 1,
            },
            MetadataColumn {
                name: "amount".into(),
                data_type: "DECIMAL(18,2)".into(),
                nullable: true,
                ordinal_position: 2,
            },
        ],
    };
    let metadata = FrozenMetadata {
        id: "metadata-orders".into(),
        version: 2,
        properties: table.properties("tenant-fixture").unwrap(),
        columns: table.columns.clone(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant-fixture".into(),
            object_version: 2,
            table_catalog: table.table_catalog.clone(),
            schema_name: table.schema_name.clone(),
            table_name: table.table_name.clone(),
            source_fingerprint: table.fingerprint().unwrap(),
        },
    };
    ReportSnapshot {
        format_version: 1,
        executor: "bi_aggregate_v1".into(),
        tenant_id: "tenant-fixture".into(),
        source_kind: "bi_report".into(),
        source_id: "report-fixture".into(),
        source_version: 3,
        report: definition::parse(&properties, &metadata.columns, "实际订单聚合").unwrap(),
        properties,
        metadata,
    }
}

fn properties() -> Value {
    json!({"metadata_object_id":"metadata-orders","dimensions":["region"],
        "measures":[{"name":"total_amount","expression":"amount","aggregate":"sum"}],"row_limit":2,
        "chart_config":{"type":"bar","title":"实际订单聚合","x_field":"region","y_field":"total_amount","legend":true}})
}

fn job(snapshot: &ReportSnapshot) -> ProductJob {
    let node = compiler::node(snapshot);
    let now = Utc::now();
    ProductJob {
        id: "job-fixture".into(),
        tenant_id: snapshot.tenant_id.clone(),
        source_kind: snapshot.source_kind.clone(),
        source_id: snapshot.source_id.clone(),
        source_version: snapshot.source_version,
        actor_id: "actor-fixture".into(),
        action: "execute_bi".into(),
        idempotency_key: "bi-fixture".into(),
        definition_snapshot: json!(snapshot),
        parameters: json!({}),
        status: ProductJobStatus::Running,
        cancel_requested: false,
        cancel_requested_by: None,
        can_retry: true,
        attempt: 1,
        lease_owner: Some("worker".into()),
        lease_fence: 1,
        lease_expires_at: None,
        result: None,
        error: None,
        reconciled_by: None,
        created_at: now,
        updated_at: now,
        started_at: Some(now),
        finished_at: None,
        nodes: vec![ProductJobNode {
            id: node.id,
            name: node.name,
            definition: node.definition,
            depends_on: node.depends_on,
            status: ProductJobNodeStatus::Running,
            attempt: 1,
            result: None,
            error: None,
            started_at: Some(now),
            finished_at: None,
        }],
    }
}

fn actual(rows: Vec<Value>) -> QueryResult {
    QueryResult {
        columns: vec![
            ColumnMeta {
                name: "region".into(),
                data_type: "Utf8".into(),
                is_numeric: false,
            },
            ColumnMeta {
                name: "total_amount".into(),
                data_type: "Decimal128".into(),
                is_numeric: true,
            },
        ],
        row_count: rows.len(),
        rows,
        suggested_chart: None,
    }
}

#[test]
fn report_sql_is_bound_to_actual_namespace_fields_and_has_an_overflow_sentinel() {
    let snapshot = snapshot(properties());
    let sql = compiler::sql(&snapshot).unwrap();
    assert_eq!(
        sql,
        "SELECT \"region\", SUM(\"amount\") AS \"total_amount\" FROM \"lake\".\"main\".\"orders\" GROUP BY \"region\" ORDER BY \"region\" LIMIT 3"
    );
    assert_eq!(
        compiler::prepare_claimed(&job(&snapshot)).unwrap(),
        vec![("report".into(), sql)]
    );
}

#[test]
fn arbitrary_expression_free_sql_shadow_alias_and_unknown_fields_are_rejected() {
    let frozen = snapshot(properties());
    for (key, value) in [
        ("sql", json!("SELECT secret FROM other")),
        ("dimensions", json!(["missing"])),
        (
            "measures",
            json!([{"name":"amount","expression":"amount","aggregate":"sum"}]),
        ),
        (
            "measures",
            json!([{"name":"total_amount","expression":"amount + 1","aggregate":"sum"}]),
        ),
    ] {
        let mut properties = properties();
        properties[key] = value;
        assert!(definition::parse(&properties, &frozen.metadata.columns, "actual").is_err());
    }
    let mut properties = properties();
    properties["measures"][0]["expression"] = json!("region");
    assert!(definition::parse(&properties, &frozen.metadata.columns, "actual").is_err());
}

#[test]
fn forged_snapshot_or_node_cannot_be_used_as_a_bi_executor_definition() {
    let frozen = snapshot(properties());
    let mut job = job(&frozen);
    job.nodes[0].definition["binding"]["table_name"] = json!("secrets");
    assert!(compiler::prepare_claimed(&job).is_err());
    job = super::tests::job(&frozen);
    job.definition_snapshot["metadata"]["binding"]["tenant_id"] = json!("other-tenant");
    assert!(compiler::prepare_claimed(&job).is_err());
}

#[test]
fn only_complete_observed_results_become_report_rows() {
    let frozen = snapshot(properties());
    let job = job(&frozen);
    let result = map_node_result(
        &job,
        actual(vec![json!({"region":"华东","total_amount":120})]),
    )
    .unwrap();
    assert_eq!(
        result["rows"],
        json!([{"region":"华东","total_amount":120}])
    );
    assert_eq!(result["complete"], true);
    assert_eq!(result["row_count"], 1);
    assert!(
        map_node_result(
            &job,
            actual(vec![json!({"region":"a","total_amount":1}); 3])
        )
        .is_err()
    );
    let mut mismatch = actual(vec![]);
    mismatch.columns[1].name = "secret".into();
    assert!(map_node_result(&job, mismatch).is_err());
    let mut nonnumeric = actual(vec![]);
    nonnumeric.columns[1].is_numeric = false;
    assert!(map_node_result(&job, nonnumeric).is_err());
}

#[test]
fn table_report_supports_count_star_but_graph_requires_one_real_dimension() {
    let frozen = snapshot(properties());
    let mut props = properties();
    props["dimensions"] = json!([]);
    props["measures"] = json!([{"name":"total_orders","expression":"*","aggregate":"count"}]);
    props["chart_config"] =
        json!({"type":"table","title":"订单总数","x_field":"","y_field":"","legend":false});
    let parsed = definition::parse(&props, &frozen.metadata.columns, "actual").unwrap();
    assert!(parsed.dimensions.is_empty());
    props["chart_config"]["type"] = json!("bar");
    assert!(definition::parse(&props, &frozen.metadata.columns, "actual").is_err());
}
