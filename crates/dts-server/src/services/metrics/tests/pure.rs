use super::{fixture, *};
use crate::services::query_svc::{ColumnMeta, QueryResult};
use chrono::Duration;
use serde_json::json;

#[test]
fn single_aggregate_actual_unicode_columns_and_unsafe_expressions() {
    let columns = fixture::metadata().columns;
    assert_eq!(
        expression::aggregate("SUM(\"销售金额\" * 2)", Some(&columns)).unwrap(),
        ("SUM".into(), Some("(\"销售金额\" * 2)".into()))
    );
    for value in [
        "SUM(missing)",
        "SUM(\"地区\")",
        "SUM(SUM(\"销售金额\"))",
        "SUM(\"销售金额\");SELECT 1",
        "COUNT(DISTINCT id)",
        "SUM(1e100)",
        "SUM((SELECT 1))",
        "AVG(read_parquet('x'))",
        "SUM(\"销售金额\") OVER ()",
    ] {
        assert!(
            expression::aggregate(value, Some(&columns)).is_err(),
            "{value}"
        );
    }
}
#[test]
fn modifier_uses_actual_types_and_escaped_constants_only() {
    let columns = fixture::metadata().columns;
    let value =
        predicate::compile("\"地区\" = 'a''; DROP TABLE orders;--'", "地区", &columns).unwrap();
    assert!(value.contains("'a''; DROP TABLE orders;--'"));
    for value in [
        "\"地区\" = 1",
        "id = 1",
        "\"地区\" = (SELECT 'a')",
        "\"地区\" = 'a';SELECT 1",
        "lower(\"地区\") = 'a'",
    ] {
        assert!(
            predicate::compile(value, "地区", &columns).is_err(),
            "{value}"
        );
    }
    assert!(predicate::compile("\"销售金额\" >= 1.25", "销售金额", &columns).is_ok());
}
#[test]
fn completed_calendar_windows_respect_month_week_leap_and_dst() {
    let window = window::completed_window(
        &PeriodDefinition {
            unit: "month".into(),
            length: 1,
            timezone: "Asia/Shanghai".into(),
        },
        "2024-03-17T00:00:00Z".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(window.start.to_rfc3339(), "2024-01-31T16:00:00+00:00");
    assert_eq!(window.end.to_rfc3339(), "2024-02-29T16:00:00+00:00");
    let week = window::completed_window(
        &PeriodDefinition {
            unit: "week".into(),
            length: 1,
            timezone: "UTC".into(),
        },
        "2026-10-09T12:00:00Z".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(week.start.to_rfc3339(), "2026-09-28T00:00:00+00:00");
    assert_eq!(week.end.to_rfc3339(), "2026-10-05T00:00:00+00:00");
    let dst = window::completed_window(
        &PeriodDefinition {
            unit: "day".into(),
            length: 1,
            timezone: "America/New_York".into(),
        },
        "2026-03-09T12:00:00Z".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(dst.end - dst.start, Duration::hours(23));
    assert!(
        window::completed_window(
            &PeriodDefinition {
                unit: "day".into(),
                length: 1,
                timezone: "invalid/zone".into()
            },
            chrono::Utc::now()
        )
        .is_err()
    );
}
#[test]
fn explicit_windows_and_frozen_periods_cannot_be_silently_overridden() {
    let mut snapshot = fixture::snapshot();
    snapshot.request.as_of = Some(chrono::Utc::now());
    assert!(
        window::resolve(
            snapshot.publication.plan.as_ref().unwrap(),
            &snapshot.request
        )
        .is_err()
    );
    snapshot.request.window_start = None;
    snapshot.request.window_end = None;
    let mut plan = fixture::plan();
    plan.measures[0].period = Some(PeriodDefinition {
        unit: "month".into(),
        length: 1,
        timezone: "Asia/Shanghai".into(),
    });
    assert!(window::resolve(&plan, &snapshot.request).is_ok());
    let mut other = plan.measures[0].clone();
    other.period.as_mut().unwrap().unit = "week".into();
    plan.measures.push(other);
    assert!(window::resolve(&plan, &snapshot.request).is_err());
}
#[test]
fn unit_formula_exact_conversion_and_incompatible_dimensions() {
    let mut money = fixture::plan();
    money.unit = Some(UnitDefinition {
        symbol: "CNY".into(),
        dimension: "money".into(),
        conversion_factor: 1.0,
    });
    money.dimension = expression::unit_dimension(money.unit.as_ref());
    let mut count = fixture::plan();
    count.dimension = expression::unit_dimension(None);
    let dependencies = vec![("revenue".into(), money), ("orders".into(), count)];
    assert!(expression::formula("revenue + orders", &dependencies).is_err());
    let (ratio, dimension) = expression::formula("revenue / orders", &dependencies).unwrap();
    assert!(ratio.contains("NULLIF"));
    assert_eq!(dimension.get("money"), Some(&1));
    assert_eq!(precision::reciprocal(1000.0).unwrap(), "0.001");
    assert_eq!(precision::reciprocal(0.01).unwrap(), "100");
    assert!(precision::reciprocal(3.0).is_err());
    assert!(precision::factor(f64::INFINITY).is_err());
}
#[test]
fn real_compiler_has_exact_namespace_half_open_window_and_overflow_sentinel() {
    let snapshot = fixture::snapshot();
    let sql = compiler::sql(&snapshot).unwrap();
    assert!(sql.contains("SUM(\"销售金额\") AS \"metric_value\""));
    assert!(sql.contains("FROM \"lake\".\"main\".\"orders\""));
    assert!(sql.contains(" >= CAST('2026-09-01T00:00:00+00:00' AS TIMESTAMPTZ)"));
    assert!(sql.ends_with("LIMIT 3"));
    assert!(!sql.contains(" / 1"));
    let mut job = fixture::job(&snapshot);
    assert_eq!(prepare_claimed(&job).unwrap(), vec![("metric".into(), sql)]);
    job.nodes[0].definition["window"]["start"] = json!("2020-01-01T00:00:00Z");
    assert!(prepare_claimed(&job).is_err());
    let mut job = fixture::job(&snapshot);
    job.definition_snapshot["metadata"]["binding"]["tenant_id"] = json!("other");
    assert!(prepare_claimed(&job).is_err());
}
fn actual(rows: Vec<serde_json::Value>) -> QueryResult {
    QueryResult {
        columns: vec![
            ColumnMeta {
                name: "地区".into(),
                data_type: "Utf8".into(),
                is_numeric: false,
            },
            ColumnMeta {
                name: "metric_value".into(),
                data_type: "Decimal128(38, 2)".into(),
                is_numeric: true,
            },
            ColumnMeta {
                name: "input_rows".into(),
                data_type: "Int64".into(),
                is_numeric: true,
            },
        ],
        row_count: rows.len(),
        rows,
        suggested_chart: None,
    }
}
#[test]
fn decimal_and_large_counts_are_preserved_without_float_conversion() {
    let value = json!({"地区":"east","metric_value":"9007199254740993123456.78","input_rows":"9007199254740993"});
    let result = map_node_result(
        &fixture::job(&fixture::snapshot()),
        actual(vec![value.clone()]),
    )
    .unwrap();
    assert_eq!(result["rows"][0], value);
    assert_eq!(result["result_states"], json!(["calculated"]));
    for value in ["1e20", "NaN", "+1.00", "01.00", "1.0", "1.001"] {
        assert!(
            map_node_result(
                &fixture::job(&fixture::snapshot()),
                actual(vec![
                    json!({"地区":"east","metric_value":value,"input_rows":1})
                ])
            )
            .is_err(),
            "{value}"
        );
    }
}
#[test]
fn empty_and_undefined_results_are_not_fake_zero_and_truncation_is_rejected() {
    let job = fixture::job(&fixture::snapshot());
    let result = map_node_result(
        &job,
        actual(vec![
            json!({"地区":"east","metric_value":null,"input_rows":0}),
            json!({"地区":"west","metric_value":null,"input_rows":10}),
        ]),
    )
    .unwrap();
    assert_eq!(
        result["result_states"],
        json!(["empty_data", "undefined_value"])
    );
    assert!(result["rows"][0]["metric_value"].is_null());
    assert!(
        map_node_result(
            &job,
            actual(vec![
                json!({"地区":"a","metric_value":"1.00","input_rows":1});
                3
            ])
        )
        .is_err()
    );
    let mut result = actual(vec![]);
    result.columns[1].data_type = "Utf8".into();
    result.columns[1].is_numeric = false;
    assert!(map_node_result(&job, result).is_err());
}
