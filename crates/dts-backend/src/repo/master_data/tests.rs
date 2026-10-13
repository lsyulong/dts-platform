// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use super::validation;

#[test]
fn strict_types_and_primary_key_rules_reject_invalid_master_values() {
    let model = json!({"columns":[{"name":"id","data_type":"INTEGER","nullable":false},{"name":"date","data_type":"DATE","nullable":false}],"key_fields":["id"]});
    for values in [
        json!({"id":"1","date":"2026-01-01"}),
        json!({"id":1,"date":"2026-02-30"}),
        json!({"id":null,"date":"2026-01-01"}),
        json!({"id":1,"date":"2026-01-01","unknown":true}),
    ] {
        assert!(
            validation::record(
                &model,
                &json!({"model_id":"model","business_key":"key","values":values})
            )
            .is_err()
        );
    }
    assert!(validation::schema(&json!({"columns":[{"name":"id","data_type":"DOUBLE","nullable":false}],"key_fields":["id"]})).is_err());
    assert!(validation::schema(&json!({"columns":[{"name":"id","data_type":"INTEGER","nullable":true}],"key_fields":["id"]})).is_err());
}

#[test]
fn decimal_and_timestamp_keys_are_normalized_before_fixed_size_fingerprint() {
    let model = json!({"columns":[{"name":"id","data_type":"DECIMAL(8,2)","nullable":false},{"name":"at","data_type":"TIMESTAMP","nullable":false}],"key_fields":["id","at"]});
    let a=validation::record(&model,&json!({"model_id":"model","business_key":"a","values":{"id":"0012.00","at":"2026-01-01T00:00:00Z"}})).expect("key a");
    let b=validation::record(&model,&json!({"model_id":"model","business_key":"b","values":{"id":"12","at":"2026-01-01T08:00:00+08:00"}})).expect("key b");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert!(validation::record(&model,&json!({"model_id":"model","business_key":"a","values":{"id":12,"at":"2026-01-01T00:00:00Z"}})).is_err());
}

#[test]
fn runtime_credentials_cannot_become_master_data_but_business_reference_ids_are_opaque() {
    assert!(validation::schema(&json!({"columns":[{"name":"password","data_type":"VARCHAR","nullable":false}],"key_fields":["password"]})).is_err());
    assert!(validation::schema(&json!({"columns":[{"name":"business_credential_ref","data_type":"VARCHAR","nullable":false}],"key_fields":["business_credential_ref"]})).is_ok());
}

#[test]
fn encoding_width_bounds_and_initial_value_are_checked_without_overflow() {
    assert!(
        validation::rule(&json!({"model_id":"model","prefix":"M","width":19,"start_value":0}))
            .is_err()
    );
    assert!(
        validation::rule(&json!({"model_id":"model","prefix":"M","width":1,"start_value":10}))
            .is_err()
    );
    assert_eq!(validation::rule(&json!({"model_id":"model","prefix":"M","width":18,"start_value":999999999999999999_i64})).expect("valid width").3,999999999999999999);
}
