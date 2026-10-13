// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use std::ops::ControlFlow;

use serde_json::json;
use sqlparser::{
    ast::{Value as SqlValue, Visit, Visitor},
    dialect::DuckDbDialect,
    parser::Parser,
};

use super::{definition, parameters, validate_definition};

fn definitions(value: serde_json::Value) -> Vec<parameters::ParameterDefinition> {
    serde_json::from_value(value).unwrap()
}

fn string_values(sql: &str) -> Vec<String> {
    struct Values(Vec<String>);
    impl Visitor for Values {
        type Break = ();
        fn pre_visit_value(&mut self, value: &SqlValue) -> ControlFlow<()> {
            if let SqlValue::SingleQuotedString(text) = value {
                self.0.push(text.clone());
            }
            ControlFlow::Continue(())
        }
    }
    let mut values = Values(vec![]);
    let statements = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
    let _: ControlFlow<()> = Visit::visit(&statements, &mut values);
    values.0
}

#[test]
fn binding_preserves_quotes_and_backslashes_as_one_literal_without_injection() {
    let definitions = definitions(json!([{"name":"name","data_type":"VARCHAR","required":true}]));
    for text in [
        "O'Reilly",
        "O''Reilly",
        "'; SELECT 99 --",
        "\\'; UNION SELECT customer_secret FROM other_table --",
        "\\\\'quoted",
        "双引号\"和中文",
    ] {
        let (_, expressions) = parameters::resolve(&definitions, &json!({"name":text})).unwrap();
        let sql = parameters::bind("SELECT $name AS value", &expressions).unwrap();
        assert_eq!(string_values(&sql), vec![text]);
        assert_eq!(Parser::parse_sql(&DuckDbDialect {}, &sql).unwrap().len(), 1);
    }
}

#[test]
fn zero_false_null_and_missing_parameters_keep_their_distinct_semantics() {
    let definitions = definitions(json!([
        {"name":"count","data_type":"BIGINT","required":true,"default_value":0},
        {"name":"enabled","data_type":"BOOLEAN","required":true,"default_value":false},
        {"name":"label","data_type":"VARCHAR","required":false,"default_value":null}
    ]));
    let (values, expressions) = parameters::resolve(&definitions, &json!({})).unwrap();
    assert_eq!(values, json!({"count":0,"enabled":false,"label":null}));
    assert!(parameters::bind("SELECT $count, $enabled, $label", &expressions).is_ok());
    assert!(parameters::resolve(&definitions, &json!({"count":null})).is_err());
    let (exact, _) = parameters::resolve(&definitions, &json!({"count":"0"})).unwrap();
    assert_eq!(exact["count"], json!("0"));
    assert!(parameters::resolve(&definitions, &json!({"count":"00"})).is_err());
    assert!(parameters::resolve(&definitions, &json!({"extra":1})).is_err());
}

#[test]
fn numeric_and_date_parameter_values_are_validated_before_literal_binding() {
    for (data_type, value) in [
        ("BIGINT", json!(1.5)),
        ("BOOLEAN", json!("false")),
        ("DATE", json!("2026-02-30")),
        ("TIMESTAMP", json!("not a timestamp")),
    ] {
        let definitions =
            definitions(json!([{"name":"value","data_type":data_type,"required":true}]));
        assert!(parameters::resolve(&definitions, &json!({"value":value})).is_err());
    }
    let definitions = definitions(json!([
        {"name":"day","data_type":"DATE","required":true},
        {"name":"instant","data_type":"TIMESTAMP","required":true}
    ]));
    let (_, expressions) = parameters::resolve(
        &definitions,
        &json!({"day":"2026-10-08","instant":"2026-10-08T08:00:00+08:00"}),
    )
    .unwrap();
    let sql = parameters::bind("SELECT $day, $instant", &expressions).unwrap();
    assert!(sql.contains("DATE '2026-10-08'"));
    assert!(sql.contains("TIMESTAMP '2026-10-08 00:00:00'"));
}

#[test]
fn rejects_credentials_and_undeclared_positional_or_identifier_parameters() {
    let ordinary = definitions(json!([{"name":"value","data_type":"VARCHAR","required":true}]));
    for text in [
        "Bearer hidden-token",
        "postgres://user:hidden@db",
        "password=hidden",
        "-----BEGIN PRIVATE KEY-----",
    ] {
        assert!(parameters::resolve(&ordinary, &json!({"value":text})).is_err());
    }
    for name in [
        "password",
        "api_key",
        "authToken",
        "private_key",
        "1invalid",
    ] {
        assert!(
            parameters::validate_definitions(&definitions(
                json!([{"name":name,"data_type":"VARCHAR","required":false}])
            ))
            .is_err()
        );
    }
    for sql in [
        "SELECT $missing",
        "SELECT ?",
        "SELECT $1",
        "SELECT 1 FROM $value",
    ] {
        assert!(parameters::inspect(sql, &ordinary).is_err());
    }
}

#[test]
fn validates_dag_dependencies_and_readonly_sql_before_can_retry() {
    let node = |id: &str, dependencies: Vec<&str>, sql: &str| json!({"id":id,"name":id,"sql":sql,"depends_on":dependencies});
    let valid = json!({"definition":{"nodes":[node("a",vec![],"SELECT 1"),node("b",vec!["a"],"SELECT 2")]}});
    let result = validate_definition("development_workflow", &valid).unwrap();
    assert_eq!(result.node_count, 2);
    assert!(result.can_retry);
    assert!(!result.binding_validated);
    assert!(!result.schedule_registered);
    for nodes in [
        vec![node("a", vec!["missing"], "SELECT 1")],
        vec![
            node("a", vec!["b"], "SELECT 1"),
            node("b", vec!["a"], "SELECT 2"),
        ],
        vec![node("a", vec![], "SELECT 1"), node("a", vec![], "SELECT 2")],
        vec![node("a", vec![], "INSERT INTO orders VALUES (1)")],
        vec![node("a", vec![], "SELECT * FROM read_csv('private.csv')")],
    ] {
        assert!(
            validate_definition(
                "development_workflow",
                &json!({"definition":{"nodes":nodes}})
            )
            .is_err()
        );
    }
}

#[test]
fn workflow_global_parameters_are_checked_across_nodes_not_faked_as_results() {
    let properties = json!({"definition":{"parameters":[{"name":"limit_value","data_type":"BIGINT","required":true}],"nodes":[
        {"id":"a","name":"A","sql":"SELECT 1","depends_on":[]},
        {"id":"b","name":"B","sql":"SELECT $limit_value","depends_on":["a"]}
    ]}});
    assert!(validate_definition("development_workflow", &properties).is_ok());
    assert!(validate_definition("sql_document", &json!({"sql":"SELECT 1","parameters":[{"name":"unused","data_type":"BIGINT","required":false}]})).is_err());
    let resource = dts_backend::ProductResource {
        id: "workflow".into(),
        tenant_id: "tenant".into(),
        kind: "development_workflow".into(),
        code: "workflow".into(),
        name: "Workflow".into(),
        description: None,
        parent_id: None,
        status: "published".into(),
        properties,
        version: 1,
        created_by: "actor".into(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        references: vec![],
    };
    let prepared = definition::prepare_resource(&resource, &json!({"limit_value":-3})).unwrap();
    assert_eq!(prepared.parameters, json!({"limit_value":-3}));
    assert_eq!(
        prepared.nodes[1].definition,
        json!({"source_sql":"SELECT $limit_value"})
    );
    assert_eq!(prepared.queries[1].1, "SELECT -3");
}
