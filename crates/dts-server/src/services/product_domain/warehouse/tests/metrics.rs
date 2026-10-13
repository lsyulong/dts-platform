use super::*;

#[test]
fn metric_expression_supports_aggregates_and_real_conditional_syntax() {
    for expression in [
        "SUM(amount)",
        "COUNT(*)",
        "SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END)",
        "SUM(amount) / NULLIF(COUNT(*), 0)",
        "COUNT(DISTINCT customer_id)",
        "SUM(amount) FILTER (WHERE status = 'paid')",
    ] {
        assert!(
            validate("atomic_metric", &atomic(expression)).is_ok(),
            "{expression}"
        );
    }
    let result = action(
        "atomic_metric",
        &atomic("SUM(amount)"),
        "validate",
        &json!({}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["binding_validated"], false);
    assert_eq!(result["executed"], false);
}

#[test]
fn metric_expression_rejects_queries_unsafe_functions_and_wrong_arity() {
    for expression in [
        "SUM(amount); DROP TABLE orders",
        "SUM(amount) AS total",
        "(SELECT 1)",
        "getenv('TOKEN')",
        "read_csv('/tmp/orders')",
        "private.sum(amount)",
        "SUM(amount) OVER ()",
        "COUNT()",
        "SUM(amount, quantity)",
        "ROUND(amount, 2, 3)",
        "1e999",
        "?",
        "unknown_fn(amount)",
    ] {
        assert!(
            validate("atomic_metric", &atomic(expression)).is_err(),
            "{expression}"
        );
    }
    let deeply_nested = format!("{}amount{}", "(".repeat(100), ")".repeat(100));
    assert!(validate("atomic_metric", &atomic(&deeply_nested)).is_err());
}

#[test]
fn derived_dimensions_are_unique_and_can_represent_global_aggregation() {
    let mut definition =
        json!({"atomic_metric_id":"metric-1", "period_id":"period-1", "dimensions":[]});
    assert!(validate("derived_metric", &definition).is_ok());
    definition["dimensions"] = json!(["Region", "region"]);
    assert!(validate("derived_metric", &definition).is_err());
    definition["dimensions"] = json!(["region; DELETE FROM users"]);
    assert!(validate("derived_metric", &definition).is_err());
}

#[test]
fn composite_requires_unique_declared_dependencies() {
    assert!(
        validate(
            "composite_metric",
            &json!({"metric_ids":[], "expression":"a / b"})
        )
        .is_err()
    );
    assert!(
        validate(
            "composite_metric",
            &json!({"metric_ids":["a", "a"], "expression":"a / b"})
        )
        .is_err()
    );
    assert!(
        validate(
            "composite_metric",
            &json!({"metric_ids":["a", "b"], "expression":"a / NULLIF(b, 0)"})
        )
        .is_ok()
    );
}

#[test]
fn metric_period_requires_real_timezone_and_integral_positive_length() {
    for timezone in ["UTC", "Asia/Shanghai", "America/New_York"] {
        assert!(
            validate(
                "metric_period",
                &json!({"unit":"month", "length":1, "timezone":timezone})
            )
            .is_ok()
        );
    }
    for length in [json!(0), json!(-1), json!(1.5), json!(10001), json!("1")] {
        assert!(
            validate(
                "metric_period",
                &json!({"unit":"day", "length":length, "timezone":"UTC"})
            )
            .is_err()
        );
    }
    for timezone in ["UTC+08:00", "Asia/DoesNotExist", "Asia/Shanghai "] {
        assert!(
            validate(
                "metric_period",
                &json!({"unit":"day", "length":1, "timezone":timezone})
            )
            .is_err()
        );
    }
    assert!(
        validate(
            "metric_period",
            &json!({"unit":"hour", "length":1, "timezone":"UTC"})
        )
        .is_err()
    );
}

#[test]
fn syntax_validation_does_not_claim_metric_calculation_or_publication() {
    for operation in ["publish", "calculate"] {
        assert!(
            action(
                "atomic_metric",
                &atomic("SUM(amount)"),
                operation,
                &json!({})
            )
            .is_err()
        );
    }
}
