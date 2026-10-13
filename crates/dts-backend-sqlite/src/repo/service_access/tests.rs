mod atomic_finish;
mod fixture;
use super::*;
use fixture::*;

#[tokio::test]
async fn actual_owner_scope_literal_search_and_total_are_consistent() {
    let f = Fixture::new(10).await;
    f.application("first", "Report 100%_ready").await;
    f.application("second", "Report 100x_ready").await;
    let mut query = f.filter("owner");
    query.q = Some("100%_".into());
    let page = f.repo.list_applications(&query).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].id, "first");
    query.offset = 1;
    assert_eq!(f.repo.list_applications(&query).await.unwrap().total, 1);
    assert!(
        f.repo
            .list_applications(&query)
            .await
            .unwrap()
            .data
            .is_empty()
    );
    assert!(matches!(
        f.repo.get_application("tenant", "outsider", "first").await,
        Err(BackendError::Forbidden(_))
    ));
    query = f.filter("other-user");
    query.tenant_id = "other".into();
    assert_eq!(f.repo.list_applications(&query).await.unwrap().total, 0);
    query = f.filter("outsider");
    query.approvals = true;
    assert!(f.repo.list_subscriptions(&query).await.is_err());
}

#[tokio::test]
async fn approval_requires_nonself_actual_permission_and_replay_keeps_original_semantics() {
    let f = Fixture::new(10).await;
    f.application("app", "App").await;
    let draft = f.subscription("app").await;
    let submit = f.decision(
        "owner",
        draft.version,
        ServiceSubscriptionDecision::Submit,
        "submit",
    );
    let pending = f
        .repo
        .decide_subscription(&submit)
        .await
        .unwrap()
        .subscription;
    let self_approve = f.decision(
        "owner",
        pending.version,
        ServiceSubscriptionDecision::Approve,
        "self",
    );
    assert!(f.repo.decide_subscription(&self_approve).await.is_err());
    let mut approve = f.decision(
        "approver",
        pending.version,
        ServiceSubscriptionDecision::Approve,
        "approve",
    );
    let result = f.repo.decide_subscription(&approve).await.unwrap();
    assert_eq!(result.subscription.status, "approved");
    assert!(f.repo.decide_subscription(&approve).await.unwrap().replayed);
    approve.reason = "different decision".into();
    assert!(matches!(
        f.repo.decide_subscription(&approve).await,
        Err(BackendError::Conflict(_))
    ));
    let mut query = f.filter("owner");
    query.q = Some("Subscription%".into());
    assert_eq!(f.repo.list_subscriptions(&query).await.unwrap().total, 1);
    sqlx::query("DELETE FROM role_permission WHERE role_id='approve-role' AND permission='services:approve'")
        .execute(&f.pool).await.unwrap();
    approve.reason = "Reviewed".into();
    assert!(f.repo.decide_subscription(&approve).await.is_err());
}

#[tokio::test]
async fn rejected_request_can_close_after_api_withdrawal() {
    let f = Fixture::new(10).await;
    f.application("app", "App").await;
    let draft = f.subscription("app").await;
    let pending = f
        .repo
        .decide_subscription(&f.decision(
            "owner",
            draft.version,
            ServiceSubscriptionDecision::Submit,
            "submit",
        ))
        .await
        .unwrap()
        .subscription;
    sqlx::query("UPDATE product_resource SET status='draft',version=version+1 WHERE tenant_id='tenant' AND kind='data_api' AND id='api'")
        .execute(&f.pool).await.unwrap();
    let rejected = f
        .repo
        .decide_subscription(&f.decision(
            "approver",
            pending.version,
            ServiceSubscriptionDecision::Reject,
            "reject",
        ))
        .await
        .unwrap();
    assert_eq!(rejected.subscription.status, "rejected");
}

#[tokio::test]
async fn quota_reservation_is_atomic_and_denials_survive_error_without_fake_permits() {
    let f = Fixture::new(1).await;
    f.application("app", "App").await;
    f.approve("app").await;
    let credential = f.credential("app", "credential").await;
    let first = f.reserve(&credential, "first");
    let second = f.reserve(&credential, "second");
    let (a, b) = tokio::join!(f.repo.reserve_call(&first), f.repo.reserve_call(&second));
    let (allowed, denied) = match (a, b) {
        (Ok(call), Err(error)) | (Err(error), Ok(call)) => (call, error),
        (a, b) => panic!("one quota winner required: {a:?}/{b:?}"),
    };
    assert!(matches!(
        denied,
        BackendError::RateLimited {
            retry_after_secs: 1..=60
        }
    ));
    let quota: i64 = sqlx::query_scalar("SELECT SUM(reserved_count) FROM service_api_minute_quota")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(quota, 1);
    let fake = f.reserve(&credential, "fake");
    let fake = ReserveServiceCallParams {
        credential_verifier: "b".repeat(64),
        ..fake
    };
    assert!(f.repo.reserve_call(&fake).await.is_err());
    let audit = f.repo.list_calls(&f.calls("owner")).await.unwrap();
    assert_eq!(audit.total, 2);
    let denial = audit
        .data
        .iter()
        .find(|event| event.reason_code.as_deref() == Some("rate_limited"))
        .unwrap();
    assert_eq!(denial.outcome, ServiceCallOutcome::Denied);
    assert!(denial.row_count.is_none() && denial.duration_ms.is_none());
    let done = f
        .repo
        .finish_call(&FinishServiceCallParams {
            call: allowed.clone(),
            outcome: ServiceCallOutcome::Succeeded,
            duration_ms: 12,
            row_count: Some(2),
            access_audit: None,
            masking_versions: None,
        })
        .await
        .unwrap();
    let replay = f
        .repo
        .finish_call(&FinishServiceCallParams {
            call: allowed.clone(),
            outcome: ServiceCallOutcome::Succeeded,
            duration_ms: 99,
            row_count: Some(2),
            access_audit: None,
            masking_versions: None,
        })
        .await
        .unwrap();
    assert_eq!(replay, done);
    assert!(
        f.repo
            .finish_call(&FinishServiceCallParams {
                call: allowed,
                outcome: ServiceCallOutcome::Failed,
                duration_ms: 12,
                row_count: None,
                access_audit: None,
                masking_versions: None,
            })
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE service_call SET row_count=8 WHERE id=?1")
            .bind(&done.id)
            .execute(&f.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn revocation_invalidates_inflight_success_but_preserves_truthful_failure_audit() {
    let f = Fixture::new(10).await;
    f.application("app", "App").await;
    f.approve("app").await;
    let credential = f.credential("app", "credential").await;
    let pending = f
        .repo
        .reserve_call(&f.reserve(&credential, "pending"))
        .await
        .unwrap();
    f.repo
        .revoke_credential(&RevokeServiceCredentialParams {
            tenant_id: "tenant".into(),
            actor_id: "owner".into(),
            application_id: "app".into(),
            credential_id: credential.id.clone(),
            expected_version: 1,
            idempotency_key: "revoke".into(),
        })
        .await
        .unwrap();
    assert!(f.repo.reauthorize_call(&pending).await.is_err());
    assert!(
        f.repo
            .finish_call(&FinishServiceCallParams {
                call: pending.clone(),
                outcome: ServiceCallOutcome::Succeeded,
                duration_ms: 5,
                row_count: Some(1),
                access_audit: None,
                masking_versions: None,
            })
            .await
            .is_err()
    );
    sqlx::query("UPDATE \"user\" SET status='deleted' WHERE id='owner'")
        .execute(&f.pool)
        .await
        .unwrap();
    let failed = f
        .repo
        .finish_call(&FinishServiceCallParams {
            call: pending,
            outcome: ServiceCallOutcome::Denied,
            duration_ms: 7,
            row_count: None,
            access_audit: None,
            masking_versions: None,
        })
        .await
        .unwrap();
    assert_eq!(failed.outcome, ServiceCallOutcome::Denied);
    assert!(matches!(
        f.repo
            .reserve_call(&f.reserve(&credential, "revoked"))
            .await,
        Err(BackendError::Forbidden(_))
    ));
    let audit = f.repo.list_calls(&f.calls("approver")).await.unwrap();
    assert_eq!(audit.total, 2);
    assert!(
        audit
            .data
            .iter()
            .any(|row| row.reason_code.as_deref() == Some("credential_unavailable"))
    );
}

#[tokio::test]
async fn origin_denial_and_cross_tenant_target_are_audited_without_disclosure() {
    let f = Fixture::new(10).await;
    f.application("app", "App").await;
    f.approve("app").await;
    let credential = f.credential("app", "credential").await;
    let mut denied = f.reserve(&credential, "origin");
    denied.origin = Some("https://unlisted.example".into());
    assert!(matches!(
        f.repo.reserve_call(&denied).await,
        Err(BackendError::Forbidden(_))
    ));
    let mut unknown = f.reserve(&credential, "unknown");
    unknown.api_id = "other-tenant-private-id".into();
    assert!(matches!(
        f.repo.reserve_call(&unknown).await,
        Err(BackendError::Forbidden(_))
    ));
    let audit = f.repo.list_calls(&f.calls("owner")).await.unwrap();
    assert_eq!(audit.total, 2);
    let event = audit
        .data
        .iter()
        .find(|event| event.id == "unknown")
        .unwrap();
    assert!(event.api_id.is_none());
    assert_eq!(
        event.target_hash,
        validation::target_hash("other-tenant-private-id")
    );
    assert_eq!(
        f.repo.list_calls(&f.calls("outsider")).await.unwrap().total,
        0
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_api_minute_quota")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
