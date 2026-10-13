#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{action, generate_ddl, validate};
use serde_json::{Value, json};
use sqlparser::ast::Statement;
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;

fn model() -> Value {
    json!({
        "layer_id": "layer-1",
        "table_name": "analytics.orders",
        "columns": [
            {"name":"OrderId", "data_type":"bigint", "nullable":true},
            {"name":"amount", "data_type":"DECIMAL(18,2)", "nullable":false}
        ],
        "primary_key": ["orderid"]
    })
}

fn atomic(expression: &str) -> Value {
    json!({"process_id":"process-1", "model_id":"model-1", "grain":"每笔订单", "expression":expression})
}

fn naming() -> Value {
    json!({"object_type":"table", "pattern":"ods_[a-z][a-z0-9_]*", "example":"ods_orders"})
}

#[test]
fn unrelated_kinds_are_left_for_other_domains() {
    assert!(validate("organization", &Value::Null).is_ok());
    assert!(
        action("organization", &Value::Null, "validate", &Value::Null)
            .unwrap()
            .is_none()
    );
}

#[test]
fn warehouse_prefix_grain_and_order_have_business_constraints() {
    assert!(
        validate(
            "warehouse_layer",
            &json!({"prefix":"ods_", "purpose":"原始数据"})
        )
        .is_ok()
    );
    for prefix in ["1ods", "ods-", "ods_; DROP TABLE users", " ", "_ods"] {
        assert!(
            validate(
                "warehouse_layer",
                &json!({"prefix":prefix, "purpose":"原始数据"})
            )
            .is_err()
        );
    }
    assert!(
        validate(
            "warehouse_process",
            &json!({"domain_id":"domain-1", "grain":"\t "})
        )
        .is_err()
    );
    assert!(validate("warehouse_category", &json!({"sort_order":1.5})).is_err());
    assert!(validate("warehouse_category", &json!({"sort_order":-1})).is_err());
}

mod metrics;
mod models;
mod standards;
