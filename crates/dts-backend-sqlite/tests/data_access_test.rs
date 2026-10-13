use chrono::{Duration, Utc};
use dts_backend::{BackendError, repo::data_access::*};
use serde_json::Value;
use sqlx::Row;

mod data_access_support;
use data_access_support::*;

#[tokio::test]
async fn real_source_scope_columns_and_version_are_mandatory_without_partial_resources() {
    let (pool, repo, target) = setup().await;
    let mut forged = save("forged", &target);
    forged.target.columns[0].data_type = "VARCHAR".into();
    assert!(repo.save_request(&forged).await.is_err());
    forged = save("forged", &target);
    forged.target.binding.table_name = "other".into();
    assert!(repo.save_request(&forged).await.is_err());
    forged = save("forged", &target);
    forged.requested_columns = vec!["ID".into()];
    assert!(repo.save_request(&forged).await.is_err());
    forged = save("forged", &target);
    forged.actor_id = "b-requester".into();
    assert!(repo.save_request(&forged).await.is_err());
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_resource WHERE kind='data_access_request'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 0);
    let real = repo
        .save_request(&save("real", &target))
        .await
        .expect("real request");
    assert_eq!(real.references.len(), 2);
    assert_eq!(real.properties["requester_id"], "a-requester");
    let typed = repo
        .get_request("a", "real")
        .await
        .expect("load")
        .expect("typed");
    assert_eq!(typed.target, target);
    let mut wrong_actor = save("real", &target);
    wrong_actor.expected_version = Some(real.version);
    wrong_actor.actor_id = "a-other".into();
    assert!(repo.save_request(&wrong_actor).await.is_err());
    assert!(
        repo.get_request("b", "real")
            .await
            .expect("tenant scope")
            .is_none()
    );
    assert!(
        sqlx::query("DELETE FROM product_resource WHERE id='metadata-a'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn approval_is_an_atomic_real_permission_separated_transition() {
    let (pool, repo, target) = setup().await;
    let draft = repo
        .save_request(&save("approval", &target))
        .await
        .expect("draft");
    let pending = repo
        .decide(&decision(
            &draft,
            "a-requester",
            DataAccessDecision::Submit,
            "submit",
        ))
        .await
        .expect("submit")
        .request;
    let mut edit = save("approval", &target);
    edit.expected_version = Some(pending.version);
    assert!(repo.save_request(&edit).await.is_err());
    permission(
        &pool,
        "a-requester",
        "self-approval",
        "data_security:approve",
        false,
    )
    .await;
    assert!(
        repo.decide(&decision(
            &pending,
            "a-requester",
            DataAccessDecision::Approve,
            "self"
        ))
        .await
        .is_err()
    );
    assert!(
        repo.decide(&decision(
            &pending,
            "a-other",
            DataAccessDecision::Approve,
            "no-permission"
        ))
        .await
        .is_err()
    );
    let mut approve = decision(
        &pending,
        "a-approver",
        DataAccessDecision::Approve,
        "approve",
    );
    approve.reason.clear();
    assert!(repo.decide(&approve).await.is_err());
    approve.reason = "Verified".into();
    approve.expires_at = Some(Utc::now() + Duration::days(2));
    assert!(repo.decide(&approve).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_access_grant")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 0);
    assert_eq!(
        repo.get_request("a", "approval")
            .await
            .expect("load")
            .expect("request")
            .resource
            .status,
        "pending"
    );
    approve.expires_at = Some(Utc::now() + Duration::hours(1));
    let result = repo.decide(&approve).await.expect("real approval");
    assert_eq!(result.request.status, "approved");
    let grant = result.grant.expect("real grant");
    assert_eq!(grant.request_version, result.request.version);
    assert_eq!(grant.columns, vec!["id"]);
    assert_eq!(grant.granted_by, "a-approver");
    let current = repo
        .active_grants("a", "a-requester", Utc::now())
        .await
        .expect("grants");
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, grant.id);
}

#[tokio::test]
async fn idempotency_replays_original_receipts_before_changed_state_or_expiry_checks() {
    let (pool, repo, target) = setup().await;
    let draft = repo
        .save_request(&save("replay", &target))
        .await
        .expect("draft");
    let pending = repo
        .decide(&decision(
            &draft,
            "a-requester",
            DataAccessDecision::Submit,
            "submit",
        ))
        .await
        .expect("pending");
    let params = decision(
        &pending.request,
        "a-approver",
        DataAccessDecision::Approve,
        "stable",
    );
    let first = repo.decide(&params).await.expect("first");
    sqlx::query(
        "UPDATE metadata_catalog SET discovery_status='missing' WHERE object_id='metadata-a'",
    )
    .execute(&pool)
    .await
    .expect("source changed");
    let replay = repo.decide(&params).await.expect("stable replay");
    assert!(replay.replayed);
    assert_eq!(replay.decision_id, first.decision_id);
    assert_eq!(
        replay.grant.expect("grant").id,
        first.grant.expect("grant").id
    );
    let mut different = params;
    different.reason = "Different semantics".into();
    assert!(matches!(
        repo.decide(&different).await,
        Err(BackendError::Conflict(_))
    ));
    permission(
        &pool,
        "a-other",
        "other-approval",
        "data_security:approve",
        false,
    )
    .await;
    different.actor_id = "a-other".into();
    assert!(matches!(
        repo.decide(&different).await,
        Err(BackendError::Conflict(_))
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM data_access_decision")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 2);
    assert!(
        sqlx::query("DELETE FROM data_access_decision")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        repo.active_grants("a", "a-requester", Utc::now())
            .await
            .expect("source invalidated grants")
            .is_empty()
    );
}

#[tokio::test]
async fn owner_withdrawal_revokes_the_grant_and_resource_together() {
    let (pool, repo, target) = setup().await;
    let approved = approved(&repo, &target, "withdraw").await;
    let params = decision(
        &approved.request,
        "a-requester",
        DataAccessDecision::Withdraw,
        "withdraw",
    );
    let result = repo.decide(&params).await.expect("withdraw");
    assert_eq!(result.request.status, "withdrawn");
    let revoked = result.grant.expect("revoked grant");
    assert!(revoked.revoked_at.is_some());
    assert_eq!(revoked.revoked_by.as_deref(), Some("a-requester"));
    assert!(
        repo.active_grants("a", "a-requester", Utc::now())
            .await
            .expect("active grants")
            .is_empty()
    );
    assert!(
        repo.decide(&decision(
            &result.request,
            "a-requester",
            DataAccessDecision::Submit,
            "resubmit"
        ))
        .await
        .is_err()
    );
    let mut edit = save("withdraw", &target);
    edit.expected_version = Some(result.request.version);
    assert!(repo.save_request(&edit).await.is_err());
    assert!(
        sqlx::query("UPDATE data_access_grant SET revoked_at=NULL,revoked_by=NULL")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM data_access_grant")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejection_can_be_corrected_but_approved_sources_and_expirations_are_fixed() {
    let (pool, repo, target) = setup().await;
    let draft = repo
        .save_request(&save("corrected", &target))
        .await
        .expect("draft");
    let pending = repo
        .decide(&decision(
            &draft,
            "a-requester",
            DataAccessDecision::Submit,
            "submit",
        ))
        .await
        .expect("submit");
    let rejected = repo
        .decide(&decision(
            &pending.request,
            "a-approver",
            DataAccessDecision::Reject,
            "reject",
        ))
        .await
        .expect("reject");
    let mut edit = save("corrected", &target);
    edit.expected_version = Some(rejected.request.version);
    edit.requested_columns.push("amount".into());
    let corrected = repo.save_request(&edit).await.expect("correction");
    assert_eq!(corrected.status, "draft");
    let submitted = repo
        .decide(&decision(
            &corrected,
            "a-requester",
            DataAccessDecision::Submit,
            "submit-new",
        ))
        .await
        .expect("resubmit");
    let accepted = repo
        .decide(&decision(
            &submitted.request,
            "a-approver",
            DataAccessDecision::Approve,
            "approve",
        ))
        .await
        .expect("approve");
    let expired = repo
        .active_grants("a", "a-requester", Utc::now() + Duration::days(1))
        .await
        .expect("expired");
    assert!(expired.is_empty());
    let revoked = repo
        .decide(&decision(
            &accepted.request,
            "a-approver",
            DataAccessDecision::Revoke,
            "revoke",
        ))
        .await
        .expect("revoke");
    assert_eq!(revoked.request.status, "revoked");
    let row =
        sqlx::query("SELECT request_semantics FROM data_access_decision WHERE action='reject'")
            .fetch_one(&pool)
            .await
            .expect("reason");
    let semantics: Value = serde_json::from_str(
        &row.try_get::<String, _>("request_semantics")
            .expect("semantics"),
    )
    .expect("JSON");
    assert_eq!(semantics["reason"], "Validated business requirement");
}
