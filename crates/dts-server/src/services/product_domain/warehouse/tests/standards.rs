use super::*;

#[test]
fn naming_regex_matches_the_whole_name_and_supports_both_payload_fields() {
    let valid = action(
        "naming_standard",
        &naming(),
        "validate",
        &json!({"sample":"ods_orders"}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(valid["matched"], true);
    let invalid = action(
        "naming_standard",
        &naming(),
        "validate",
        &json!({"name":"xods_orders"}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(invalid["valid"], false);
    let suffix = action(
        "naming_standard",
        &naming(),
        "validate",
        &json!({"sample":"ods_orders!"}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(suffix["matched"], false);
}

#[test]
fn naming_rejects_invalid_complex_regex_and_inconsistent_examples() {
    for pattern in ["[", "(a)\\1", "(?<=a)b", "[a-z]{100000000}"] {
        assert!(
            validate(
                "naming_standard",
                &json!({"object_type":"table", "pattern":pattern})
            )
            .is_err()
        );
    }
    let mut config = naming();
    config["example"] = json!("orders");
    assert!(validate("naming_standard", &config).is_err());
}

#[test]
fn enumeration_requires_unique_codes_and_meanings() {
    let duplicate =
        json!({"enum_values":[{"value":"A", "label":"甲"},{"value":"A", "label":"乙"}]});
    assert!(validate("enum_standard", &duplicate).is_err());
    assert!(
        validate(
            "enum_standard",
            &json!({"enum_values":[{"value":" A", "label":"甲"}]})
        )
        .is_err()
    );
    assert!(
        validate(
            "enum_standard",
            &json!({"enum_values":[{"value":"A", "label":" "}]})
        )
        .is_err()
    );
    assert!(validate("enum_standard", &json!({"enum_values":[]})).is_err());
    assert!(
        validate(
            "enum_standard",
            &json!({"enum_values":[{"value":"A", "label":"甲"},{"value":"a", "label":"乙"}]})
        )
        .is_ok()
    );
}

#[test]
fn field_types_and_decimal_bounds_are_whitelisted() {
    for data_type in ["DECIMAL(1,0)", "decimal (38, 38)", "VARCHAR", "TIMESTAMPTZ"] {
        assert!(
            validate(
                "field_standard",
                &json!({"data_type":data_type, "definition":"业务口径", "nullable":false})
            )
            .is_ok()
        );
    }
    for data_type in [
        "DECIMAL(0,0)",
        "DECIMAL(39,2)",
        "DECIMAL(18,19)",
        "DECIMAL(18)",
        "DECIMAL(18,2,0)",
        "BIGINT); DROP TABLE users; --",
        "VARCHAR[]",
        "custom_type",
    ] {
        assert!(
            validate(
                "field_standard",
                &json!({"data_type":data_type, "definition":"业务口径"})
            )
            .is_err()
        );
    }
    assert!(
        validate(
            "field_standard",
            &json!({"data_type":"BIGINT", "definition":"业务口径", "nullable":"false"})
        )
        .is_err()
    );
}

#[test]
fn unit_conversion_uses_real_factors_and_preserves_zero() {
    let source = json!({"symbol":"mm", "dimension":"length", "conversion_factor":0.001});
    let target = json!({"symbol":"m", "dimension":"length"});
    let result = action(
        "unit_standard",
        &source,
        "convert",
        &json!({"value":1200.0, "target":target}),
    )
    .unwrap()
    .unwrap();
    assert!((result["converted_value"].as_f64().unwrap() - 1.2).abs() < 1e-12);
    let zero = action(
        "unit_standard",
        &source,
        "convert",
        &json!({"value":0, "target":target}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(zero["converted_value"], 0.0);
    let negative = action(
        "unit_standard",
        &source,
        "convert",
        &json!({"value":-1000, "target":target}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(negative["converted_value"], -1.0);
}

#[test]
fn unit_conversion_rejects_invalid_factors_and_other_dimensions() {
    for factor in [json!(0), json!(-1), Value::Null, json!("1")] {
        assert!(
            validate(
                "unit_standard",
                &json!({"symbol":"m", "dimension":"length", "conversion_factor":factor})
            )
            .is_err()
        );
    }
    let source = json!({"symbol":"m", "dimension":"length"});
    assert!(
        action(
            "unit_standard",
            &source,
            "convert",
            &json!({"value":1,
        "target":{"symbol":"kg", "dimension":"mass"}})
        )
        .is_err()
    );
    assert!(action("unit_standard", &source, "convert", &json!({"value":1})).is_err());
}

#[test]
fn conversion_handles_intermediate_limits_but_rejects_unrepresentable_results() {
    let source = json!({"symbol":"small", "dimension":"scale", "conversion_factor":1e-300});
    let target = json!({"symbol":"large", "dimension":"scale", "conversion_factor":1e300});
    let result = action(
        "unit_standard",
        &source,
        "convert",
        &json!({"value":1e300, "target":target}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result["converted_value"], 1e-300);
    assert!(
        action(
            "unit_standard",
            &target,
            "convert",
            &json!({"value":1e300, "target":source})
        )
        .is_err()
    );
    assert!(
        action(
            "unit_standard",
            &source,
            "convert",
            &json!({"value":1e-300, "target":target})
        )
        .is_err()
    );
}
