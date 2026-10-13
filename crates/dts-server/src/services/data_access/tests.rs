#![allow(clippy::unwrap_used)]

use super::*;
use dts_backend::repo::{
    data_access::*,
    metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};

fn target() -> DataAccessTarget {
    DataAccessTarget {
        metadata_id: "meta-1".into(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant-a".into(),
            object_version: 1,
            table_catalog: "warehouse".into(),
            schema_name: "main".into(),
            table_name: "orders".into(),
            source_fingerprint: "a".repeat(64),
        },
        columns: vec![
            MetadataColumn {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                ordinal_position: 1,
            },
            MetadataColumn {
                name: "amount".into(),
                data_type: "DOUBLE".into(),
                nullable: true,
                ordinal_position: 2,
            },
            MetadataColumn {
                name: "secret".into(),
                data_type: "VARCHAR".into(),
                nullable: true,
                ordinal_position: 3,
            },
        ],
    }
}

fn used(sql: &str) -> Result<Vec<String>, crate::models::error::AppError> {
    let parsed = ast::parse(sql)?;
    Ok(ast::columns(&parsed, &target())?.into_iter().collect())
}

#[test]
fn field_binding_checks_projection_predicate_and_order_columns() {
    assert_eq!(
        used("SELECT id FROM warehouse.main.orders WHERE amount>0 ORDER BY secret LIMIT 20")
            .unwrap(),
        ["amount", "id", "secret"]
    );
    assert_eq!(
        used("SELECT o.id FROM warehouse.main.orders o WHERE o.amount > 0 ORDER BY o.id").unwrap(),
        ["amount", "id"]
    );
    assert!(used("SELECT id FROM warehouse.main.orders WHERE not_collected=1").is_err());
    assert!(used("SELECT id FROM warehouse.main.orders ORDER BY other.id").is_err());
}

#[test]
fn physical_locator_must_match_all_three_real_components() {
    let parsed = ast::parse("SELECT id FROM warehouse.main.orders").unwrap();
    assert!(ast::matches(&parsed, &target()));
    for sql in [
        "SELECT id FROM other.main.orders",
        "SELECT id FROM warehouse.other.orders",
        "SELECT id FROM warehouse.main.other",
    ] {
        assert!(!ast::matches(&ast::parse(sql).unwrap(), &target()));
    }
    assert!(ast::parse("SELECT id FROM orders").is_err());
    assert!(ast::parse("SELECT id FROM main.orders").is_err());
    assert!(ast::matches(
        &ast::parse("SELECT ID FROM \"WAREHOUSE\".\"MAIN\".\"ORDERS\"").unwrap(),
        &target()
    ));
}

#[test]
fn wildcard_count_and_constant_queries_require_full_schema() {
    for sql in [
        "SELECT * FROM warehouse.main.orders",
        "SELECT o.* FROM warehouse.main.orders o",
        "SELECT COUNT(*) FROM warehouse.main.orders",
        "SELECT 1 FROM warehouse.main.orders",
    ] {
        assert_eq!(used(sql).unwrap(), ["amount", "id", "secret"]);
    }
    assert_eq!(
        used("SELECT COUNT(id) FROM warehouse.main.orders").unwrap(),
        ["id"]
    );
    assert!(used("SELECT * EXCLUDE(secret) FROM warehouse.main.orders").is_err());
}

#[test]
fn unsupported_query_paths_fail_closed_instead_of_skipping_field_checks() {
    for sql in [
        "WITH x AS(SELECT id FROM warehouse.main.orders) SELECT * FROM x",
        "SELECT id FROM warehouse.main.orders UNION SELECT id FROM warehouse.main.orders",
        "SELECT id FROM warehouse.main.orders JOIN warehouse.main.orders b USING(id)",
        "SELECT * FROM read_csv('secret.csv')",
        "SELECT * FROM (SELECT id FROM warehouse.main.orders)",
        "SELECT id FROM warehouse.main.orders WHERE id IN(SELECT id FROM warehouse.main.orders)",
        "SELECT getenv('TOKEN') FROM warehouse.main.orders",
        "SELECT ROW_NUMBER() OVER() FROM warehouse.main.orders",
        "SELECT id FROM warehouse.main.orders TABLESAMPLE(10 ROWS)",
        "SELECT id FROM warehouse.main.orders WHERE id=?",
    ] {
        assert!(used(sql).is_err(), "{sql}");
    }
}

#[test]
fn aliases_cannot_shadow_real_columns_or_hide_unauthorized_expression_inputs() {
    assert_eq!(
        used("SELECT amount+1 AS computed FROM warehouse.main.orders ORDER BY computed").unwrap(),
        ["amount"]
    );
    assert!(used("SELECT id AS secret FROM warehouse.main.orders ORDER BY secret").is_err());
    assert!(used("SELECT id AS x,amount AS x FROM warehouse.main.orders").is_err());
    assert_eq!(
        used("SELECT secret AS x FROM warehouse.main.orders ORDER BY x").unwrap(),
        ["secret"]
    );
}

#[test]
fn hostile_literals_are_parsed_as_data_and_query_text_is_not_rendered_again() {
    let sql = "SELECT id FROM warehouse.main.orders WHERE secret = 'x''; DROP TABLE orders; --'";
    assert_eq!(used(sql).unwrap(), ["id", "secret"]);
    assert!(used("SELECT id FROM warehouse.main.orders; DROP TABLE orders").is_err());
    assert_eq!(validation::query_hash(sql), validation::query_hash(sql));
    assert_ne!(
        validation::query_hash(sql),
        validation::query_hash("SELECT id FROM warehouse.main.orders")
    );
}

#[test]
fn limits_and_offsets_cannot_execute_expressions_or_unbounded_values() {
    assert!(used("SELECT id FROM warehouse.main.orders LIMIT 0 OFFSET 1000000").is_ok());
    for sql in [
        "SELECT id FROM warehouse.main.orders LIMIT 10001",
        "SELECT id FROM warehouse.main.orders OFFSET 1000001",
        "SELECT id FROM warehouse.main.orders LIMIT id",
        "SELECT id FROM warehouse.main.orders LIMIT -1",
        "SELECT id FROM warehouse.main.orders LIMIT 1.5",
        "SELECT id FROM warehouse.main.orders LIMIT ALL",
        "SELECT id FROM warehouse.main.orders LIMIT /*actual syntax*/ ALL OFFSET 0",
    ] {
        assert!(used(sql).is_err(), "unexpectedly accepted: {sql}");
    }
    assert!(used("SELECT id FROM warehouse.main.orders WHERE secret='LIMIT ALL' LIMIT 1").is_ok());
}

#[test]
fn request_column_selection_is_real_canonical_unique_and_explicitly_frozen() {
    let target = target();
    assert_eq!(
        input::requested_columns(None, &target.columns).unwrap(),
        ["id", "amount", "secret"]
    );
    assert_eq!(
        input::requested_columns(Some(&serde_json::json!([])), &target.columns).unwrap(),
        ["id", "amount", "secret"]
    );
    assert_eq!(
        input::requested_columns(Some(&serde_json::json!(["ID"])), &target.columns).unwrap(),
        ["id"]
    );
    assert!(
        input::requested_columns(Some(&serde_json::json!(["ghost"])), &target.columns).is_err()
    );
    assert!(
        input::requested_columns(Some(&serde_json::json!(["id", "ID"])), &target.columns).is_err()
    );
    assert!(validation::target(&target, "tenant-b", &["id".into()]).is_err());
}

#[test]
fn single_table_grouping_uses_the_same_real_column_permissions() {
    assert_eq!(used("SELECT id,SUM(amount) AS total_amount FROM warehouse.main.orders GROUP BY id ORDER BY id LIMIT 100").unwrap(),["amount","id"]);
    assert_eq!(
        used("SELECT COUNT(id) AS total_id FROM warehouse.main.orders GROUP BY secret").unwrap(),
        ["id", "secret"]
    );
    assert!(used("SELECT id FROM warehouse.main.orders GROUP BY ROLLUP(id)").is_err());
    assert!(used("SELECT id FROM warehouse.main.orders GROUP BY unknown").is_err());
}

#[test]
fn decision_receipt_freezes_input_semantics_without_requiring_current_state() {
    let mut params = DecideDataAccessParams {
        tenant_id: "tenant-a".into(),
        request_id: "request".into(),
        expected_version: 2,
        actor_id: "reviewer".into(),
        decision: DataAccessDecision::Approve,
        reason: "必要访问".into(),
        expires_at: Some(chrono::Utc::now() - chrono::Duration::days(1)),
        idempotency_key: "decision-1".into(),
    };
    // Past effective expiry must not prevent replay of a successful old receipt.
    let original = validation::decision_request(&params).unwrap();
    params.reason = "different".into();
    assert_ne!(validation::decision_request(&params).unwrap(), original);
    params.reason.clear();
    assert!(validation::decision_request(&params).is_err());
}

mod quality;
mod query_service;
