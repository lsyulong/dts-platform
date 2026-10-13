mod standard_governance_support;
use dts_backend::{BackendError, repo::standard_governance::*};
use serde_json::json;
use standard_governance_support::*;

#[tokio::test]
async fn immutable_revisions_survive_new_drafts_and_old_withdrawal_does_not_bump_source() {
    let (pool, repo) = setup().await;
    let first = publish(&pool, &repo, "field_standard", "field", vec![]).await;
    assert_eq!(first.revision_version, 2);
    let revised = repo
        .transition(&StandardTransitionParams {
            scope: scope("field_standard", "field", 2),
            command: StandardCommand::Revise,
        })
        .await
        .unwrap();
    assert_eq!(revised.resource.version, 3);
    assert_eq!(
        repo.revision("a", "field_standard", "field", 2)
            .await
            .unwrap()
            .unwrap()
            .state,
        "active"
    );
    let withdrawn = repo
        .transition(&StandardTransitionParams {
            scope: scope("field_standard", "field", 3),
            command: StandardCommand::Withdraw {
                revision_version: 2,
                expected_state_version: 1,
                reason: "Version retired".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(withdrawn.resource.version, 3);
    assert_eq!(withdrawn.revision.unwrap().state_version, 2);
    assert!(
        repo.revision("b", "field_standard", "field", 2)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        sqlx::query("UPDATE standard_revision SET payload='{}' WHERE tenant_id='a'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("DELETE FROM product_resource WHERE tenant_id='a' AND kind='field_standard' AND id='field'").execute(&pool).await.is_err());
    let mut other = scope("field_standard", "field", 3);
    other.actor_id = "other".into();
    assert!(matches!(
        repo.transition(&StandardTransitionParams {
            scope: other,
            command: StandardCommand::Revise
        })
        .await,
        Err(BackendError::Forbidden(_))
    ));
}
#[tokio::test]
async fn actual_child_revisions_and_hashes_are_checked_before_atomic_publish() {
    let (pool, repo) = setup().await;
    let unit = publish(&pool, &repo, "unit_standard", "unit", vec![]).await;
    let before = source(&pool, "field_standard", "field").await;
    let mut properties = before.properties.clone();
    properties["unit_id"] = json!("unit");
    properties["unit_version"] = json!(2);
    let audit_before = count(&pool, "product_resource_audit").await;
    let mut child = unit.reference();
    child.definition_hash = "0".repeat(64);
    let failed = repo
        .transition(&StandardTransitionParams {
            scope: scope("field_standard", "field", 1),
            command: StandardCommand::Publish {
                expected_properties: before.properties.clone(),
                normalized_properties: properties.clone(),
                children: vec![child],
            },
        })
        .await;
    assert!(matches!(failed, Err(BackendError::Conflict(_))));
    assert_eq!(source(&pool, "field_standard", "field").await.version, 1);
    assert_eq!(count(&pool, "product_resource_audit").await, audit_before);
    let published = repo
        .transition(&StandardTransitionParams {
            scope: scope("field_standard", "field", 1),
            command: StandardCommand::Publish {
                expected_properties: before.properties,
                normalized_properties: properties,
                children: vec![unit.reference()],
            },
        })
        .await
        .unwrap();
    assert_eq!(published.revision.unwrap().children[0].revision_version, 2);
}
#[tokio::test]
async fn model_binding_has_real_column_coverage_and_concurrent_cas() {
    let (pool, repo) = setup().await;
    let standard = publish(&pool, &repo, "field_standard", "field", vec![]).await;
    let mut invalid = bind_params(&pool, standard.reference()).await;
    invalid.properties["columns"][0]["data_type"] = json!("VARCHAR");
    assert!(repo.bind_model(&invalid).await.is_err());
    assert_eq!(source(&pool, "data_model", "model").await.version, 1);
    let valid = bind_params(&pool, standard.reference()).await;
    let (one, two) = tokio::join!(repo.bind_model(&valid), repo.bind_model(&valid));
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(count(&pool, "standard_model_binding").await, 1);
    assert_eq!(source(&pool, "data_model", "model").await.version, 2);
    assert!(
        repo.bindings(&StandardConsumerLookup {
            tenant_id: "b".into(),
            actor_id: "other".into(),
            model_id: "model".into()
        })
        .await
        .is_err()
    );
}
#[tokio::test]
async fn actual_model_publication_gate_replays_only_the_same_receipt() {
    let (pool, repo) = setup().await;
    let standard = publish(&pool, &repo, "field_standard", "field", vec![]).await;
    repo.bind_model(&bind_params(&pool, standard.reference()).await)
        .await
        .unwrap();
    let claim = prepared(&pool, &repo).await;
    assert!(!claim.needs_reconciliation);
    let params = finish_params(
        &claim,
        StandardModelEngineOutcome::Published { receipt: receipt() },
    );
    let done = repo.finish_model_publication(&params).await.unwrap();
    assert_eq!(done.resource.status, "published");
    assert_eq!(done.resource.version, claim.claim_version + 1);
    assert_eq!(done.standards_state, ModelStandardsState::Validated);
    assert_eq!(
        done.publication.properties["standards"]["dependencies"],
        json!(claim.dependencies)
    );
    let replay = repo.finish_model_publication(&params).await.unwrap();
    assert_eq!(replay.publication.id, done.publication.id);
    assert!(matches!(
        repo.finish_model_publication(&finish_params(
            &claim,
            StandardModelEngineOutcome::Failed {
                reason: "Different engine outcome".into()
            }
        ))
        .await,
        Err(BackendError::Conflict(_))
    ));
}
#[tokio::test]
async fn withdrawn_revision_during_physical_work_preserves_effect_as_unknown() {
    let (pool, repo) = setup().await;
    let standard = publish(&pool, &repo, "field_standard", "field", vec![]).await;
    repo.bind_model(&bind_params(&pool, standard.reference()).await)
        .await
        .unwrap();
    let claim = prepared(&pool, &repo).await;
    repo.transition(&StandardTransitionParams {
        scope: scope("field_standard", "field", 2),
        command: StandardCommand::Withdraw {
            revision_version: 2,
            expected_state_version: 1,
            reason: "Unsafe standard".into(),
        },
    })
    .await
    .unwrap();
    let actual = receipt();
    let done = repo
        .finish_model_publication(&finish_params(
            &claim,
            StandardModelEngineOutcome::Published {
                receipt: actual.clone(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(done.resource.status, "publish_unknown");
    assert_eq!(done.publication.status, "unknown");
    assert_eq!(done.publication.properties["receipt"], actual);
    assert_eq!(done.standards_state, ModelStandardsState::Invalidated);
}
#[tokio::test]
async fn authority_loss_after_engine_effect_does_not_discard_the_receipt() {
    let (pool, repo) = setup().await;
    let claim = prepared(&pool, &repo).await;
    sqlx::query(
        "DELETE FROM role_permission WHERE role_id='role-a' AND permission='models:execute'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let done = repo
        .finish_model_publication(&finish_params(
            &claim,
            StandardModelEngineOutcome::Published { receipt: receipt() },
        ))
        .await
        .unwrap();
    assert_eq!(done.resource.status, "publish_unknown");
    assert_eq!(done.standards_state, ModelStandardsState::Invalidated);
    assert!(!done.publication.properties["receipt"].is_null());
}
#[tokio::test]
async fn stale_takeover_fences_a_late_old_receipt_without_overwriting_the_new_model() {
    let (pool, repo) = setup().await;
    let old = prepared(&pool, &repo).await;
    assert!(
        repo.begin_model_publication(&BeginStandardModelPublicationParams {
            tenant_id: "a".into(),
            actor_id: "actor".into(),
            model_id: "model".into(),
            expected_model_version: old.claim_version,
            expected_properties: old.resource.properties.clone(),
            binding_version: None,
            binding_hash: None,
            dependencies: vec![]
        })
        .await
        .is_err()
    );
    sqlx::query("UPDATE product_resource SET updated_at=?1 WHERE tenant_id='a' AND kind='data_model' AND id='model'").bind((chrono::Utc::now()-chrono::Duration::minutes(11)).to_rfc3339()).execute(&pool).await.unwrap();
    let new = prepared(&pool, &repo).await;
    assert!(new.needs_reconciliation);
    assert_eq!(new.claim_version, old.claim_version + 1);
    let completed = repo
        .finish_model_publication(&finish_params(
            &new,
            StandardModelEngineOutcome::Published { receipt: receipt() },
        ))
        .await
        .unwrap();
    let late = repo
        .finish_model_publication(&finish_params(
            &old,
            StandardModelEngineOutcome::Published { receipt: receipt() },
        ))
        .await
        .unwrap();
    assert_eq!(late.publication.status, "unknown");
    assert_eq!(late.resource.status, "published");
    assert_eq!(late.resource.version, completed.resource.version);
    assert_eq!(
        source(&pool, "data_model", "model").await.version,
        completed.resource.version
    );
    assert_eq!(count(&pool, "standard_model_gate").await, 2);
}
#[tokio::test]
async fn declared_pins_cannot_bypass_missing_typed_binding_and_unknown_is_reconciliation_only() {
    let (pool, repo) = setup().await;
    let mut properties = source(&pool, "data_model", "model").await.properties;
    properties["standard_coverage"] = json!("all_fields");
    sqlx::query("UPDATE product_resource SET properties=?1 WHERE tenant_id='a' AND kind='data_model' AND id='model'").bind(properties.to_string()).execute(&pool).await.unwrap();
    assert!(
        repo.begin_model_publication(&BeginStandardModelPublicationParams {
            tenant_id: "a".into(),
            actor_id: "actor".into(),
            model_id: "model".into(),
            expected_model_version: 1,
            expected_properties: properties.clone(),
            binding_version: None,
            binding_hash: None,
            dependencies: vec![]
        })
        .await
        .is_err()
    );
    assert_eq!(count(&pool, "standard_model_gate").await, 0);
    properties
        .as_object_mut()
        .unwrap()
        .remove("standard_coverage");
    sqlx::query("UPDATE product_resource SET properties=?1,status='publish_unknown' WHERE tenant_id='a' AND kind='data_model' AND id='model'").bind(properties.to_string()).execute(&pool).await.unwrap();
    assert!(prepared(&pool, &repo).await.needs_reconciliation);
}
#[tokio::test]
async fn inspection_requires_current_collector_proof_and_records_only_schema_evidence() {
    let (pool, repo) = setup().await;
    successful_model(&pool, &repo).await;
    let source = collected_metadata(&pool).await;
    let snapshot = repo.inspection_snapshot(&source).await.unwrap();
    let result = inspection_result(&snapshot);
    let mut fake = result.clone();
    fake.data_values_evaluated = true;
    assert!(
        repo.record_inspection(&RecordStandardInspectionParams {
            source: source.clone(),
            snapshot_hash: snapshot.snapshot_hash.clone(),
            result: fake
        })
        .await
        .is_err()
    );
    assert_eq!(count(&pool, "standard_inspection").await, 0);
    let recorded = repo
        .record_inspection(&RecordStandardInspectionParams {
            source: source.clone(),
            snapshot_hash: snapshot.snapshot_hash.clone(),
            result: result.clone(),
        })
        .await
        .unwrap();
    assert_eq!(recorded.evaluation_scope, "collected_schema");
    assert!(!recorded.unit_semantics_evaluated);
    assert!(
        sqlx::query("UPDATE standard_inspection SET payload='{}' WHERE tenant_id='a'")
            .execute(&pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.columns[0].data_type','VARCHAR') WHERE tenant_id='a' AND kind='metadata_object' AND id='metadata'").execute(&pool).await.unwrap();
    assert!(repo.inspection_snapshot(&source).await.is_err());
    assert!(
        repo.record_inspection(&RecordStandardInspectionParams {
            source,
            snapshot_hash: snapshot.snapshot_hash.clone(),
            result: inspection_result(&snapshot)
        })
        .await
        .is_err()
    );
}
