// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{BackendError, repo::master_data::*};
use serde_json::json;

mod master_data_support;
use master_data_support::*;

#[tokio::test]
async fn published_model_is_frozen_control_schema_and_cannot_be_edited() {
    let (pool, repo) = setup().await;
    let properties = json!({"columns":[{"name":"id","data_type":"VARCHAR","nullable":false}],"key_fields":["id"]});
    let model = repo
        .save_model(&write("second", None, properties.clone()))
        .await
        .expect("model");
    let published = repo
        .publish_model(&scope(&model, "a-owner"))
        .await
        .expect("publish");
    assert!(!published.physical_published);
    assert_eq!(published.model_version, 2);
    assert_eq!(published.schema_snapshot, properties);
    assert!(
        repo.save_model(&write("second", Some(2), properties))
            .await
            .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM master_model_schema WHERE model_id='second'")
            .fetch_one(&pool)
            .await
            .expect("snapshot count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn business_and_actual_schema_keys_are_independently_unique_and_invalid_writes_roll_back() {
    let (pool, repo) = setup().await;
    repo.save_record(&record("first", "customer", 1, "one", None))
        .await
        .expect("record");
    assert!(matches!(
        repo.save_record(&record("duplicate-key", "other", 1, "two", None))
            .await,
        Err(BackendError::Conflict(_))
    ));
    assert!(matches!(
        repo.save_record(&record("duplicate-business", "customer", 2, "two", None))
            .await,
        Err(BackendError::Conflict(_))
    ));
    let mut invalid = record("invalid", "invalid", 3, "three", None);
    invalid.fields.properties["values"]["id"] = json!("3");
    assert!(matches!(
        repo.save_record(&invalid).await,
        Err(BackendError::ConstraintViolation(_))
    ));
    invalid.fields.properties["values"]["id"] = json!(3);
    invalid.fields.properties["model_id"] = json!("other-tenant-model");
    assert!(repo.save_record(&invalid).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE kind='master_data_record'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_resource_audit WHERE resource_kind='master_data_record'",
    )
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn approval_requires_distinct_real_approver_exact_permission_and_reason() {
    let (_pool, repo) = setup().await;
    let pending = submitted(&repo, "record").await;
    for actor in ["a-owner", "a-other", "b-approver"] {
        assert!(
            repo.transition_record(&MasterRecordActionParams {
                scope: scope(&pending, actor),
                action: MasterRecordAction::Approve,
                reason: "approved".into()
            })
            .await
            .is_err()
        );
    }
    assert!(
        repo.transition_record(&MasterRecordActionParams {
            scope: scope(&pending, "a-approver"),
            action: MasterRecordAction::Approve,
            reason: "".into()
        })
        .await
        .is_err()
    );
    assert!(
        repo.save_record(&record(
            "record",
            "record",
            1,
            "edited while pending",
            Some(pending.version)
        ))
        .await
        .is_err()
    );
    let approved = repo
        .transition_record(&MasterRecordActionParams {
            scope: scope(&pending, "a-approver"),
            action: MasterRecordAction::Approve,
            reason: "source verified".into(),
        })
        .await
        .expect("approve");
    assert_eq!(approved.status, "approved");
    assert!(
        repo.transition_record(&MasterRecordActionParams {
            scope: scope(&pending, "a-approver"),
            action: MasterRecordAction::Reject,
            reason: "stale".into()
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn approved_edit_creates_draft_revision_preserves_approved_snapshot_and_locks_identity() {
    let (pool, repo) = setup().await;
    let pending = submitted(&repo, "record").await;
    let approved = repo
        .transition_record(&MasterRecordActionParams {
            scope: scope(&pending, "a-approver"),
            action: MasterRecordAction::Approve,
            reason: "verified".into(),
        })
        .await
        .expect("approve");
    let revised = repo
        .save_record(&record(
            "record",
            "record",
            1,
            "revised",
            Some(approved.version),
        ))
        .await
        .expect("new draft");
    assert_eq!(revised.status, "draft");
    assert_eq!(revised.version, approved.version + 1);
    let snapshots = repo
        .approved_records(&MasterApprovedFilter {
            tenant_id: "a".into(),
            model_id: "model".into(),
            limit: 20,
            offset: 0,
        })
        .await
        .expect("approved snapshot");
    assert_eq!(snapshots.total, 1);
    assert_eq!(snapshots.data[0].record_version, approved.version);
    assert_eq!(snapshots.data[0].values["name"], "original");
    assert!(
        repo.save_record(&record(
            "record",
            "changed",
            1,
            "changed",
            Some(revised.version)
        ))
        .await
        .is_err()
    );
    assert!(
        repo.save_record(&record(
            "record",
            "record",
            2,
            "changed",
            Some(revised.version)
        ))
        .await
        .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM master_record_revision WHERE record_id='record'")
            .fetch_one(&pool)
            .await
            .expect("history");
    assert_eq!(count, 4);
    assert!(
        repo.delete_draft(&DeleteMasterResourceParams {
            scope: scope(&revised, "a-owner"),
            kind: "master_data_record".into()
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn rejected_record_can_be_corrected_and_resubmitted_with_history() {
    let (_pool, repo) = setup().await;
    let pending = submitted(&repo, "record").await;
    let rejected = repo
        .transition_record(&MasterRecordActionParams {
            scope: scope(&pending, "a-approver"),
            action: MasterRecordAction::Reject,
            reason: "name invalid".into(),
        })
        .await
        .expect("reject");
    let draft = repo
        .save_record(&record(
            "record",
            "record",
            1,
            "corrected",
            Some(rejected.version),
        ))
        .await
        .expect("correct");
    let pending = repo
        .transition_record(&MasterRecordActionParams {
            scope: scope(&draft, "a-owner"),
            action: MasterRecordAction::Submit,
            reason: "corrected".into(),
        })
        .await
        .expect("resubmit");
    assert_eq!(pending.status, "pending");
}

#[tokio::test]
async fn atomic_code_allocation_is_unique_idempotent_and_cannot_reconfigure_used_rule() {
    let (_pool, repo) = setup().await;
    let rule = rule(&repo, "rule", 3, 1).await;
    let a = MasterAllocateParams {
        scope: scope(&rule, "a-owner"),
        idempotency_key: "one".into(),
    };
    let b = MasterAllocateParams {
        scope: scope(&rule, "a-owner"),
        idempotency_key: "two".into(),
    };
    let (a, b) = tokio::join!(repo.allocate_code(&a), repo.allocate_code(&b));
    let a = a.expect("a");
    let b = b.expect("b");
    assert_ne!(a.code, b.code);
    assert_eq!(
        repo.allocate_code(&MasterAllocateParams {
            scope: scope(&rule, "a-owner"),
            idempotency_key: "one".into()
        })
        .await
        .expect("idempotent")
        .code,
        a.code
    );
    assert!(
        repo.allocate_code(&MasterAllocateParams {
            scope: scope(&rule, "a-other"),
            idempotency_key: "one".into()
        })
        .await
        .is_err()
    );
    assert!(
        repo.save_rule(&write(
            "rule",
            Some(rule.version),
            json!({"model_id":"model","prefix":"X","width":3,"start_value":1})
        ))
        .await
        .is_err()
    );
    assert!(
        repo.delete_draft(&DeleteMasterResourceParams {
            scope: scope(&rule, "a-owner"),
            kind: "master_code_rule".into()
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn code_width_exhaustion_does_not_overflow_or_consume_additional_sequence() {
    let (pool, repo) = setup().await;
    let rule = rule(&repo, "rule", 1, 9).await;
    let allocated = repo
        .allocate_code(&MasterAllocateParams {
            scope: scope(&rule, "a-owner"),
            idempotency_key: "last".into(),
        })
        .await
        .expect("last");
    assert_eq!(allocated.code, "M9");
    assert!(
        repo.allocate_code(&MasterAllocateParams {
            scope: scope(&rule, "a-owner"),
            idempotency_key: "too-far".into()
        })
        .await
        .is_err()
    );
    let next: i64 =
        sqlx::query_scalar("SELECT next_value FROM master_code_sequence WHERE rule_id='rule'")
            .fetch_one(&pool)
            .await
            .expect("next");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM master_code_allocation WHERE rule_id='rule'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(next, 9);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn every_master_write_rejects_disabled_actor_or_tenant() {
    let (pool, repo) = setup().await;
    sqlx::query("UPDATE \"user\" SET status='disabled' WHERE id='a-owner'")
        .execute(&pool)
        .await
        .expect("disable actor");
    assert!(
        repo.save_record(&record("record", "record", 1, "name", None))
            .await
            .is_err()
    );
    sqlx::query("UPDATE \"user\" SET status='active' WHERE id='a-owner'")
        .execute(&pool)
        .await
        .expect("enable actor");
    sqlx::query("UPDATE tenant SET status='disabled' WHERE id='a'")
        .execute(&pool)
        .await
        .expect("disable tenant");
    assert!(
        repo.save_record(&record("record", "record", 1, "name", None))
            .await
            .is_err()
    );
}
