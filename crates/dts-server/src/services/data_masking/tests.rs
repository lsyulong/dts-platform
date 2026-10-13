use super::{
    PrivacyKey,
    policy::{group_definition, parameters, rule_definition},
    types::{BoundRule, RuleDefinition},
};
use crate::{
    models::auth::AuthIdentity,
    services::{
        data_access::{AuthorizedQuery, masking_outputs},
        query_svc::{ColumnMeta, QueryResult},
    },
};
use dts_backend::repo::{
    data_access::DataAccessTarget,
    metadata_catalog::{MetadataColumn, MetadataEngineBinding},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

#[cfg(feature = "sqlite")]
mod sqlite;

fn target() -> DataAccessTarget {
    DataAccessTarget {
        metadata_id: "metadata-1".into(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "tenant".into(),
            object_version: 4,
            table_catalog: "lake".into(),
            schema_name: "main".into(),
            table_name: "people".into(),
            source_fingerprint: "a".repeat(64),
        },
        columns: vec![
            MetadataColumn {
                name: "email".into(),
                data_type: "VARCHAR".into(),
                nullable: true,
                ordinal_position: 1,
            },
            MetadataColumn {
                name: "age".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                ordinal_position: 2,
            },
        ],
    }
}
fn rule(method: &str, parameters: Value) -> BoundRule {
    BoundRule {
        rule_id: "rule-1".into(),
        rule_version: 2,
        groups: vec![("group-1".into(), 1)],
        target: target(),
        definition: RuleDefinition {
            object_id: "metadata-1".into(),
            column_name: "email".into(),
            method: method.into(),
            parameters,
            metadata_version: Some(4),
            column_data_type: Some("VARCHAR".into()),
            source_fingerprint: Some("a".repeat(64)),
        },
    }
}
fn result(value: Value) -> QueryResult {
    QueryResult {
        columns: vec![ColumnMeta {
            name: "contact".into(),
            data_type: "VARCHAR".into(),
            is_numeric: false,
        }],
        rows: vec![json!({"contact":value})],
        row_count: 1,
        suggested_chart: None,
    }
}

#[test]
fn lineage_accepts_alias_and_wildcard_but_denies_inference() {
    let sensitive: BTreeSet<_> = ["email".to_string()].into_iter().collect();
    assert_eq!(
        masking_outputs(
            "SELECT email AS contact FROM lake.main.people",
            &target(),
            &sensitive
        )
        .unwrap(),
        vec![("contact".into(), "email".into())]
    );
    assert_eq!(
        masking_outputs("SELECT * FROM lake.main.people", &target(), &sensitive).unwrap(),
        vec![("email".into(), "email".into())]
    );
    for sql in [
        "SELECT lower(email) FROM lake.main.people",
        "SELECT age FROM lake.main.people WHERE email='secret'",
        "SELECT email AS contact FROM lake.main.people ORDER BY contact",
        "SELECT email FROM lake.main.people GROUP BY email",
        "SELECT email FROM lake.main.people ORDER BY 1",
        "SELECT DISTINCT email FROM lake.main.people",
    ] {
        assert!(
            masking_outputs(sql, &target(), &sensitive).is_err(),
            "{sql}"
        );
    }
    assert!(
        masking_outputs(
            "SELECT email FROM lake.main.people WHERE age>18 ORDER BY age",
            &target(),
            &sensitive
        )
        .is_ok()
    );
}

#[test]
fn policy_fields_and_sensitive_json_parameters_are_strict() {
    assert!(group_definition(&json!({"member_ids":["user","user"]})).is_err());
    assert!(group_definition(&json!({"member_ids":[]})).is_err());
    assert!(group_definition(&json!({"member_ids":["user"],"trusted":true})).is_err());
    assert!(
        rule_definition(
            &json!({"object_id":"obj","column_name":"email","method":"hash","secret":"bad"})
        )
        .is_err()
    );
    assert!(parameters(&rule("hash", json!({"salt":"bad"})).definition).is_err());
    assert!(parameters(&rule("partial", json!({"keep_start":33})).definition).is_err());
    assert!(parameters(&rule("partial", json!({"keep_start":-1})).definition).is_err());
    assert_eq!(
        parameters(&rule("partial", json!({"keep_start":1,"keep_end":0})).definition).unwrap(),
        (1, 0)
    );
}

#[test]
fn receipt_key_signatures_are_bound_to_bytes_and_key() {
    let key = PrivacyKey::from_bytes([7; 32]);
    let signature = key.sign(b"server result hash");
    assert!(key.verify(b"server result hash", &signature));
    assert!(!key.verify(b"other result hash", &signature));
    assert!(!PrivacyKey::from_bytes([8; 32]).verify(b"server result hash", &signature));
    assert!(!key.verify(b"server result hash", "not-a-tag"));
}

#[cfg(feature = "sqlite")]
async fn service() -> super::MaskingService {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_lazy("sqlite::memory:")
        .unwrap();
    super::MaskingService::new(
        Arc::new(dts_backend_sqlite::SqliteProductResourceRepo::new(
            pool.clone(),
        )),
        Arc::new(dts_backend_sqlite::SqliteMetadataCatalogRepo::new(
            pool.clone(),
        )),
        Arc::new(dts_backend_sqlite::SqliteUserRepo::new(pool)),
        PrivacyKey::from_bytes([9; 32]),
    )
}
#[cfg(feature = "sqlite")]
fn plan(rule: BoundRule) -> super::MaskingPlan {
    let actor = AuthIdentity {
        sub: "user".into(),
        tenant_id: "tenant".into(),
        username: "user".into(),
    };
    super::MaskingPlan {
        query: AuthorizedQuery::masking_test_token(
            &actor,
            "SELECT email AS contact FROM lake.main.people",
        ),
        policy_hash: "b".repeat(64),
        policy_versions: vec![],
        fields: vec![super::types::OutputMask {
            output_name: "contact".into(),
            rule,
        }],
    }
}

#[test]
fn opaque_plan_only_matches_actual_tenant_actor_and_exact_sql() {
    let actor = AuthIdentity {
        sub: "reader".into(),
        tenant_id: "tenant".into(),
        username: "reader".into(),
    };
    let plan = super::MaskingPlan {
        query: AuthorizedQuery::masking_test_token(&actor, "SELECT 1"),
        policy_hash: "a".repeat(64),
        policy_versions: vec![],
        fields: vec![],
    };
    let sql = plan.query.sql();
    assert!(plan.is_bound_to("tenant", "reader", sql));
    assert!(!plan.is_bound_to("other", "reader", sql));
    assert!(!plan.is_bound_to("tenant", "other", sql));
    assert!(!plan.is_bound_to("tenant", "reader", &format!("{sql} ")));
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn actual_values_are_masked_null_kept_and_receipt_detects_tampering() {
    let service = service().await;
    let plan = plan(rule("redact", json!({})));
    let masked = service.apply_output(&plan, result(json!(100))).unwrap();
    assert_eq!(masked.rows[0]["contact"], "***");
    assert_eq!(masked.columns[0].data_type, "VARCHAR");
    assert!(!masked.columns[0].is_numeric);
    let receipt = service.seal_output(&plan, &masked).unwrap();
    assert_eq!(receipt.result_hash, super::output::hash(&masked).unwrap());
    assert_ne!(
        receipt.result_hash,
        super::output::hash(&result(json!("changed"))).unwrap()
    );
    assert!(
        service
            .apply_output(&plan, result(Value::Null))
            .unwrap()
            .rows[0]["contact"]
            .is_null()
    );
    let mut invalid = result(json!("value"));
    invalid.row_count = 2;
    assert!(service.apply_output(&plan, invalid).is_err());
    assert!(serde_json::from_value::<super::MaskingReceipt>(json!({"version":1,"tenant_id":"tenant",
        "query_hash":"a".repeat(64),"policy_hash":"b".repeat(64),"result_hash":"c".repeat(64),"signature":"tag","trusted":true})).is_err());
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn unicode_partial_never_preserves_the_complete_value_and_hmac_is_scoped() {
    let service = service().await;
    let partial = rule("partial", json!({"keep_start":1,"keep_end":1}));
    assert_eq!(
        service
            .mask_value("tenant", &partial, &json!("张三先生"))
            .unwrap(),
        "张***生"
    );
    assert_eq!(
        service
            .mask_value("tenant", &partial, &json!("张三"))
            .unwrap(),
        "***"
    );
    assert_eq!(
        service.mask_value("tenant", &partial, &json!("")).unwrap(),
        "***"
    );
    assert!(service.mask_value("tenant", &partial, &json!(12)).is_err());
    let hash = rule("hash", json!({}));
    let original = json!("private-value");
    let masked = service.mask_value("tenant", &hash, &original).unwrap();
    assert_eq!(
        masked,
        service.mask_value("tenant", &hash, &original).unwrap()
    );
    assert_ne!(
        masked,
        service
            .mask_value("other-tenant", &hash, &original)
            .unwrap()
    );
    assert_ne!(
        masked,
        service.mask_value("tenant", &hash, &json!(123)).unwrap()
    );
    assert!(!masked.as_str().unwrap().contains("private-value"));
}
