use super::*;
use crate::SqliteProductResourceRepo;
use dts_backend::{
    CreateResourceParams, ProductResourceRepo, ResourceReference, ResourceWrite,
    UpdateResourceParams,
};
use serde_json::json;
mod fixture;
use fixture::Fixture;

#[tokio::test]
async fn snapshots_are_exact_immutable_and_receipts_replay_after_source_changes() {
    let f = Fixture::new().await;
    let params = f.save("document", 1).await;
    let first = f.repo.save(&params).await.unwrap();
    assert!(first.created && !first.replayed);
    let detail = f.detail("document", &first.revision.id).await;
    assert_eq!(
        detail.snapshot.properties,
        json!({"sql":"SELECT 1 AS value","parameters":[]})
    );
    assert_eq!(detail.snapshot.version, 1);
    assert_eq!(
        first.revision.snapshot_hash,
        validation::snapshot_hash(&detail.snapshot).unwrap()
    );
    f.edit(
        "document",
        1,
        json!({"sql":"SELECT 2 AS value","parameters":[]}),
    )
    .await;
    let replay = f
        .repo
        .save(&SaveDevelopmentRevisionParams {
            validation: None,
            ..params
        })
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.revision.id, first.revision.id);
    assert!(
        sqlx::query("UPDATE development_revision SET reason='changed'")
            .execute(&f.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM development_revision_receipt")
            .execute(&f.pool)
            .await
            .is_err()
    );
    assert!(
        SqliteProductResourceRepo::new(f.pool.clone())
            .delete("tenant", "sql_document", "document", 2, "admin")
            .await
            .is_err()
    );
}
#[tokio::test]
async fn real_permissions_identity_and_receipt_actor_semantics_are_rechecked() {
    let f = Fixture::new().await;
    let mut p = f.save("document", 1).await;
    p.scope.actor_id = "reader".into();
    assert!(matches!(
        f.repo.save(&p).await,
        Err(BackendError::Forbidden(_))
    ));
    p.scope.actor_id = "admin".into();
    f.repo.save(&p).await.unwrap();
    p.scope.actor_id = "admin2".into();
    assert!(matches!(
        f.repo.save(&p).await,
        Err(BackendError::Conflict(_))
    ));
    p.scope.actor_id = "admin".into();
    p.reason = "other".into();
    assert!(matches!(
        f.repo.save(&p).await,
        Err(BackendError::Conflict(_))
    ));
    let mut s = f.scope("document");
    s.tenant_id = "other".into();
    assert!(matches!(
        f.repo.current(&s).await,
        Err(BackendError::Forbidden(_))
    ));
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='admin'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.repo.current(&f.scope("document")).await,
        Err(BackendError::Forbidden(_))
    ));
}
#[tokio::test]
async fn witness_and_unique_source_version_prevent_fabrication_or_duplicate_history() {
    let f = Fixture::new().await;
    let p = f.save("document", 1).await;
    let wrong = ValidatedDevelopmentDefinition::from_validated_snapshot(
        &f.repo.current(&f.scope("workflow")).await.unwrap(),
    )
    .unwrap();
    assert!(
        f.repo
            .save(&SaveDevelopmentRevisionParams {
                scope: f.scope("document"),
                expected_version: 1,
                reason: "save".into(),
                idempotency_key: uuid::Uuid::new_v4().to_string(),
                validation: Some(wrong)
            })
            .await
            .is_err()
    );
    let a = f.repo.save(&p).await.unwrap();
    let b = f.repo.save(&f.save("document", 1).await).await.unwrap();
    assert_eq!(a.revision.id, b.revision.id);
    assert!(!b.created);
    assert_ne!(a.receipt_id, b.receipt_id);
    assert_eq!(
        f.repo
            .history(&DevelopmentRevisionFilter {
                scope: f.scope("document"),
                limit: 1,
                offset: 0
            })
            .await
            .unwrap()
            .total,
        1
    );
}
#[tokio::test]
async fn restore_is_cas_new_draft_and_published_definition_cannot_be_overwritten() {
    let f = Fixture::new().await;
    let saved = f.repo.save(&f.save("document", 1).await).await.unwrap();
    f.edit(
        "document",
        1,
        json!({"sql":"SELECT 2 AS value","parameters":[]}),
    )
    .await;
    let p = f.restore("document", 2, &saved.revision.id).await;
    let mut other = f.restore("document", 2, &saved.revision.id).await;
    other.reason = "second restore".into();
    let (a, b) = tokio::join!(f.repo.restore(&p), f.repo.restore(&other));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let actual = f.repo.current(&f.scope("document")).await.unwrap();
    assert_eq!(actual.status, "draft");
    assert_eq!(actual.version, 3);
    assert_eq!(actual.properties["sql"], "SELECT 1 AS value");
    assert_eq!(actual.created_by, "admin");
    f.status("document", "published").await;
    assert!(
        f.repo
            .restore(&f.restore("document", 3, &saved.revision.id).await)
            .await
            .is_err()
    );
    f.status("document", "inactive").await;
    let result = f
        .repo
        .restore(&f.restore("document", 3, &saved.revision.id).await)
        .await
        .unwrap();
    assert_eq!(result.resource.version, 4);
    assert!(result.requires_publish);
}
#[tokio::test]
async fn restored_frozen_references_are_currently_revalidated_and_failure_rolls_back() {
    let f = Fixture::new().await;
    let resource = SqliteProductResourceRepo::new(f.pool.clone());
    resource
        .create(&CreateResourceParams {
            id: "model".into(),
            tenant_id: "tenant".into(),
            kind: "data_model".into(),
            actor_id: "admin".into(),
            fields: ResourceWrite {
                code: "model".into(),
                name: "Model".into(),
                description: None,
                parent_id: None,
                status: "draft".into(),
                properties: json!({}),
                references: vec![],
            },
        })
        .await
        .unwrap();
    let mut current = f.repo.current(&f.scope("document")).await.unwrap();
    current.properties["model_id"] = json!("model");
    current.references = vec![ResourceReference {
        field: "model_id".into(),
        target_kind: "data_model".into(),
        target_id: "model".into(),
    }];
    resource
        .update(
            "tenant",
            "sql_document",
            "document",
            &UpdateResourceParams {
                expected_version: 1,
                actor_id: "admin".into(),
                fields: fixture::fields(&current),
            },
        )
        .await
        .unwrap();
    let saved = f.repo.save(&f.save("document", 2).await).await.unwrap();
    f.edit("document", 2, json!({"sql":"SELECT 9","parameters":[]}))
        .await;
    resource
        .delete("tenant", "data_model", "model", 1, "admin")
        .await
        .unwrap();
    assert!(
        f.repo
            .restore(&f.restore("document", 3, &saved.revision.id).await)
            .await
            .is_err()
    );
    assert_eq!(
        f.repo.current(&f.scope("document")).await.unwrap().version,
        3
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM development_revision_receipt WHERE action='restore'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn failed_audit_is_atomic_and_history_and_detail_are_single_source() {
    let f = Fixture::new().await;
    sqlx::raw_sql("CREATE TRIGGER fail_revision_audit BEFORE INSERT ON product_resource_audit WHEN json_extract(NEW.summary,'$.action')='save_version' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").execute(&f.pool).await.unwrap();
    assert!(f.repo.save(&f.save("document", 1).await).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM development_revision")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_revision_audit")
        .execute(&f.pool)
        .await
        .unwrap();
    let saved = f.repo.save(&f.save("workflow", 1).await).await.unwrap();
    assert!(
        f.repo
            .detail(&ReadDevelopmentRevisionParams {
                scope: f.scope("document"),
                revision_id: saved.revision.id
            })
            .await
            .is_err()
    );
    assert_eq!(
        f.repo
            .history(&DevelopmentRevisionFilter {
                scope: f.scope("document"),
                limit: 20,
                offset: 0
            })
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn restore_receipt_failure_rolls_back_content_version_and_audit() {
    let f = Fixture::new().await;
    let saved = f.repo.save(&f.save("document", 1).await).await.unwrap();
    f.edit(
        "document",
        1,
        json!({"sql":"SELECT 2 AS value","parameters":[]}),
    )
    .await;
    sqlx::raw_sql("CREATE TRIGGER fail_restore_receipt BEFORE INSERT ON development_revision_receipt WHEN NEW.action='restore' BEGIN SELECT RAISE(ABORT,'receipt unavailable'); END;")
        .execute(&f.pool).await.unwrap();
    assert!(
        f.repo
            .restore(&f.restore("document", 2, &saved.revision.id).await)
            .await
            .is_err()
    );
    let current = f.repo.current(&f.scope("document")).await.unwrap();
    assert_eq!(current.version, 2);
    assert_eq!(current.properties["sql"], "SELECT 2 AS value");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_resource_audit WHERE json_extract(summary,'$.action')='restore_version'")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn restore_original_receipt_replays_without_overwriting_a_later_draft() {
    let f = Fixture::new().await;
    let saved = f.repo.save(&f.save("document", 1).await).await.unwrap();
    f.edit("document", 1, json!({"sql":"SELECT 2","parameters":[]}))
        .await;
    let params = f.restore("document", 2, &saved.revision.id).await;
    let first = f.repo.restore(&params).await.unwrap();
    f.edit("document", 3, json!({"sql":"SELECT 4","parameters":[]}))
        .await;
    let replay = f.repo.restore(&params).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(first.receipt_id, replay.receipt_id);
    assert_eq!(replay.resource.version, 3);
    let actual = f.repo.current(&f.scope("document")).await.unwrap();
    assert_eq!(actual.version, 4);
    assert_eq!(actual.properties["sql"], "SELECT 4");
}
