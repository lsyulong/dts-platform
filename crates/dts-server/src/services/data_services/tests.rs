// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{compiler, map_node_result, schema, types::*};
use crate::services::query_svc::{ColumnMeta, QueryResult};
use chrono::Utc;
use dts_backend::repo::{
    metadata_catalog::{MetadataColumn, MetadataEngineBinding, MetadataTable},
    product_job::*,
};
use serde_json::{Value, json};
use sqlparser::{ast::*, dialect::DuckDbDialect, parser::Parser};

fn metadata() -> ApiMetadata {
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
    ApiMetadata {
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
    }
}

fn properties(sql: &str) -> Value {
    json!({"metadata_object_id":"metadata-orders", "sql":sql,
        "parameters":[], "response_schema":[], "row_limit":2, "rate_limit":60})
}

fn snapshot(mut properties: Value) -> ApiJobSnapshot {
    let metadata = metadata();
    let publication = ApiPublication {
        format_version: 1,
        executor: "data_api_detail_v1".into(),
        tenant_id: "tenant-fixture".into(),
        source_id: "api-fixture".into(),
        source_version: 3,
        published_by: "actor-fixture".into(),
        published_at: Utc::now().to_rfc3339(),
        api: compiler::definition(&properties, &metadata).unwrap(),
        metadata: metadata.clone(),
    };
    properties["response_schema"] = json!(publication.api.response_schema);
    properties["publication"] = json!(publication);
    ApiJobSnapshot {
        format_version: 1,
        executor: "data_api_detail_v1".into(),
        tenant_id: "tenant-fixture".into(),
        source_kind: "data_api".into(),
        source_id: "api-fixture".into(),
        source_version: 3,
        properties,
        metadata,
        publication,
    }
}

fn job(snapshot: &ApiJobSnapshot, parameters: Value) -> ProductJob {
    let node = compiler::node(&snapshot.publication);
    let now = Utc::now();
    ProductJob {
        id: "job-fixture".into(),
        tenant_id: snapshot.tenant_id.clone(),
        source_kind: snapshot.source_kind.clone(),
        source_id: snapshot.source_id.clone(),
        source_version: snapshot.source_version,
        actor_id: "actor-fixture".into(),
        action: "execute_data_api".into(),
        idempotency_key: "api-fixture".into(),
        definition_snapshot: json!(snapshot),
        parameters,
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
                name: "amount".into(),
                data_type: "Decimal128(18, 2)".into(),
                is_numeric: true,
            },
        ],
        row_count: rows.len(),
        rows,
        suggested_chart: None,
    }
}

#[test]
fn direct_fields_freeze_actual_schema_and_bound_limit_without_running_sql() {
    let props = properties("SELECT region, amount FROM lake.main.orders ORDER BY region");
    let api = compiler::definition(&props, &metadata()).unwrap();
    assert_eq!(
        api.compiled_sql,
        "SELECT \"region\", \"amount\" FROM lake.main.orders ORDER BY region LIMIT 3"
    );
    assert_eq!(api.response_schema[0].data_type, "VARCHAR");
    assert!(!api.response_schema[0].nullable);
    assert_eq!(api.response_schema[1].data_type, "DECIMAL(18,2)");
    assert!(api.response_schema[1].nullable);
    let lower = compiler::definition(
        &properties("SELECT region FROM lake.main.orders LIMIT 1"),
        &metadata(),
    )
    .unwrap();
    assert!(lower.compiled_sql.ends_with("LIMIT 1"));
    let capped = compiler::definition(
        &properties("SELECT region FROM lake.main.orders LIMIT 500 OFFSET 5"),
        &metadata(),
    )
    .unwrap();
    assert!(capped.compiled_sql.ends_with("LIMIT 3 OFFSET 5"));
}

#[test]
fn declared_schema_must_match_actual_projection_including_alias_order_type_and_nullable() {
    let mut props = properties("SELECT region AS area, amount FROM lake.main.orders");
    let schema = compiler::definition(&props, &metadata())
        .unwrap()
        .response_schema;
    props["response_schema"] = json!(schema);
    assert!(compiler::definition(&props, &metadata()).is_ok());
    for (field, value) in [
        ("name", json!("other")),
        ("data_type", json!("BIGINT")),
        ("nullable", json!(true)),
    ] {
        let mut mismatch = props.clone();
        mismatch["response_schema"][0][field] = value;
        assert!(compiler::definition(&mismatch, &metadata()).is_err());
    }
    props["response_schema"] = json!([schema[1], schema[0]]);
    assert!(compiler::definition(&props, &metadata()).is_err());
}

#[test]
fn foreign_sources_functions_complex_shapes_unknown_columns_and_unbounded_limits_are_rejected() {
    for sql in [
        "SELECT region FROM main.orders",
        "SELECT region FROM other.main.orders",
        "SELECT * FROM lake.main.orders",
        "SELECT missing FROM lake.main.orders",
        "SELECT sum(amount) FROM lake.main.orders",
        "SELECT upper(region) FROM lake.main.orders",
        "SELECT region AS amount FROM lake.main.orders",
        "SELECT region AS area, region AS area FROM lake.main.orders",
        "SELECT region FROM read_parquet('https://example.com/table')",
        "SELECT region FROM lake.main.orders o JOIN lake.main.orders x ON o.region=x.region",
        "WITH x AS (SELECT region FROM lake.main.orders) SELECT region FROM x",
        "SELECT region FROM lake.main.orders WHERE region IN (SELECT region FROM lake.main.orders)",
        "SELECT region FROM lake.main.orders WHERE upper(region)='A'",
        "SELECT region FROM lake.main.orders ORDER BY amount+1",
        "SELECT region FROM lake.main.orders LIMIT :cap",
        "SELECT region FROM lake.main.orders LIMIT ALL",
        "SELECT region FROM lake.main.orders LIMIT 10001",
        "SELECT region FROM lake.main.orders OFFSET 1000001",
        "SELECT region FROM lake.main.orders; SELECT 1",
        "SELECT DISTINCT region FROM lake.main.orders",
        "FROM lake.main.orders SELECT region",
        "DELETE FROM lake.main.orders",
    ] {
        assert!(
            compiler::definition(&properties(sql), &metadata()).is_err(),
            "accepted {sql}"
        );
    }
}

#[test]
fn typed_parameter_binding_keeps_quotes_as_one_value_in_ast() {
    let mut props = properties("SELECT region, amount FROM lake.main.orders WHERE region=$area");
    props["parameters"] = json!([{"name":"area","data_type":"VARCHAR","required":true}]);
    let snapshot = snapshot(props);
    let value = "O'Reilly' OR TRUE --";
    let job = job(&snapshot, json!({"area":value}));
    let sql = compiler::prepare_claimed(&job).unwrap().remove(0).1;
    let statements = Parser::parse_sql(&DuckDbDialect {}, &sql).unwrap();
    assert_eq!(statements.len(), 1);
    let Statement::Query(query) = &statements[0] else {
        panic!("not query")
    };
    let SetExpr::Select(select) = &*query.body else {
        panic!("not select")
    };
    let Expr::BinaryOp { right, op, .. } = select.selection.as_ref().unwrap() else {
        panic!("not equality")
    };
    assert_eq!(*op, BinaryOperator::Eq);
    let Expr::Value(sqlvalue) = &**right else {
        panic!("not literal")
    };
    assert_eq!(
        sqlvalue.value,
        sqlparser::ast::Value::SingleQuotedString(value.into())
    );
    assert!(compiler::prepare_claimed(&super::tests::job(&snapshot, json!({"area":7}))).is_err());
    assert!(
        compiler::prepare_claimed(&super::tests::job(
            &snapshot,
            json!({"area":value,"extra":1})
        ))
        .is_err()
    );
}

#[test]
fn unbounded_limit_tokens_reject_across_comments_without_matching_strings_or_quoted_aliases() {
    for sql in [
        "SELECT region FROM lake.main.orders LIMIT ALL",
        "SELECT region FROM lake.main.orders LIMIT /* bounded? */ ALL",
        "SELECT region FROM lake.main.orders LIMIT -- unbounded\n ALL",
    ] {
        assert!(compiler::definition(&properties(sql), &metadata()).is_err());
    }
    for sql in [
        "SELECT region FROM lake.main.orders WHERE region='LIMIT ALL'",
        "SELECT region FROM lake.main.orders WHERE region='LIMIT /* x */ ALL'",
        "SELECT region AS \"LIMIT ALL\" FROM lake.main.orders",
        "SELECT region FROM lake.main.orders /* LIMIT ALL */ LIMIT 1",
    ] {
        assert!(
            compiler::definition(&properties(sql), &metadata()).is_ok(),
            "rejected {sql}"
        );
    }
}

#[test]
fn publication_and_node_snapshot_cannot_be_substituted_by_claimed_job() {
    let frozen = snapshot(properties("SELECT region, amount FROM lake.main.orders"));
    let mut actual = job(&frozen, json!({}));
    assert!(compiler::prepare_claimed(&actual).is_ok());
    actual.nodes[0].definition["source_sql"] = json!("SELECT secret FROM other");
    assert!(compiler::prepare_claimed(&actual).is_err());
    actual = job(&frozen, json!({}));
    actual.definition_snapshot["publication"]["source_version"] = json!(4);
    assert!(compiler::prepare_claimed(&actual).is_err());
    actual = job(&frozen, json!({}));
    actual.definition_snapshot["metadata"]["binding"]["tenant_id"] = json!("foreign");
    assert!(compiler::prepare_claimed(&actual).is_err());
    actual = job(&frozen, json!({}));
    actual.definition_snapshot["properties"]["sql"] = json!("SELECT region FROM lake.main.orders");
    assert!(compiler::prepare_claimed(&actual).is_err());
    actual = job(&frozen, json!({}));
    actual.can_retry = false;
    assert!(compiler::prepare_claimed(&actual).is_err());
}

#[test]
fn real_output_requires_complete_rows_exact_types_and_no_truncated_success() {
    let frozen = snapshot(properties("SELECT region, amount FROM lake.main.orders"));
    let job = job(&frozen, json!({}));
    let observed = map_node_result(
        &job,
        actual(vec![json!({"region":"华东","amount":"120.25"})]),
    )
    .unwrap();
    assert_eq!(observed["row_count"], 1);
    assert_eq!(observed["complete"], true);
    assert_eq!(observed["external_credentials_ready"], false);
    for rows in [
        vec![json!({"region":"a","amount":"1.00"}); 3],
        vec![json!({"region":null,"amount":"1.00"})],
        vec![json!({"region":"a","amount":"1.001"})],
        vec![json!({"region":"a","amount":1.2})],
        vec![json!({"region":"a","amount":"1.00","secret":"extra"})],
    ] {
        assert!(map_node_result(&job, actual(rows)).is_err());
    }
    let mut masked = actual(vec![json!({"region":"a","amount":"hmac:v1:x"})]);
    masked.columns[1].data_type = "VARCHAR".into();
    masked.columns[1].is_numeric = false;
    assert!(map_node_result(&job, masked).is_err());
    let mut missing = actual(vec![]);
    missing.columns[0].name = "other".into();
    assert!(map_node_result(&job, missing).is_err());
}

#[test]
fn scalar_values_respect_integer_width_decimal_scale_and_timezone_contract() {
    let field = |data_type: &str| ApiResponseField {
        name: "value".into(),
        data_type: data_type.into(),
        nullable: false,
        description: String::new(),
    };
    assert!(schema::actual_value(&field("TINYINT"), &json!(127)));
    assert!(!schema::actual_value(&field("TINYINT"), &json!(128)));
    assert!(!schema::actual_value(&field("UTINYINT"), &json!(-1)));
    assert!(schema::actual_value(
        &field("BIGINT"),
        &json!(i64::MIN.to_string())
    ));
    assert!(schema::actual_value(
        &field("DECIMAL(4,2)"),
        &json!("99.99")
    ));
    assert!(!schema::actual_value(
        &field("DECIMAL(4,2)"),
        &json!("100.00")
    ));
    assert!(!schema::actual_value(
        &field("DECIMAL(4,2)"),
        &json!("0.001")
    ));
    assert!(schema::actual_type(
        "TIMESTAMP",
        "Timestamp(Microsecond, None)"
    ));
    assert!(!schema::actual_type(
        "TIMESTAMP",
        "Timestamp(Microsecond, Some(\"UTC\"))"
    ));
    assert!(!schema::actual_type("DECIMAL(18,2)", "Decimal128(18,3)"));
}
