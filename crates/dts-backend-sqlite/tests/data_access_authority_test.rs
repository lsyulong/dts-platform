use chrono::Utc;
use dts_backend::repo::data_access::*;
use serde_json::json;

mod data_access_support;
use data_access_support::*;

#[tokio::test]
async fn live_grants_fail_closed_on_metadata_locator_schema_or_version_tampering() {
    let (pool, repo, target) = setup().await;
    approved(&repo, &target, "bound-proof").await;
    let original: String =
        sqlx::query_scalar("SELECT properties FROM product_resource WHERE id='metadata-a'")
            .fetch_one(&pool)
            .await
            .expect("original properties");
    sqlx::query("UPDATE product_resource SET properties=json_set(properties,'$.table_name','forged') WHERE id='metadata-a'")
        .execute(&pool).await.expect("tampered locator");
    assert!(
        repo.active_grants("a", "a-requester", Utc::now())
            .await
            .expect("locator mismatch")
            .is_empty()
    );
    sqlx::query("UPDATE product_resource SET properties=?1 WHERE id='metadata-a'")
        .bind(&original)
        .execute(&pool)
        .await
        .expect("restore source");
    sqlx::query("UPDATE metadata_catalog SET columns_snapshot=json_set(columns_snapshot,'$[0].data_type','VARCHAR') WHERE object_id='metadata-a'")
        .execute(&pool).await.expect("tampered internal columns");
    assert!(
        repo.active_grants("a", "a-requester", Utc::now())
            .await
            .expect("schema mismatch")
            .is_empty()
    );
    sqlx::query("UPDATE metadata_catalog SET columns_snapshot=?1,object_version=2 WHERE object_id='metadata-a'").bind(json!(target.columns).to_string()).execute(&pool).await.expect("next source version");
    sqlx::query("UPDATE product_resource SET version=2 WHERE id='metadata-a'")
        .execute(&pool)
        .await
        .expect("next resource version");
    assert!(
        repo.active_grants("a", "a-requester", Utc::now())
            .await
            .expect("frozen version mismatch")
            .is_empty()
    );
}

#[tokio::test]
async fn owner_and_approval_pages_count_and_page_the_same_scoped_filter() {
    let (_, repo, target) = setup().await;
    repo.save_request(&save("one", &target)).await.expect("one");
    let mut second = save("two", &target);
    second.actor_id = "a-other".into();
    repo.save_request(&second).await.expect("two");
    let filter = DataAccessRequestFilter {
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        approvals: false,
        status: Some("draft".into()),
        limit: 1,
        offset: 0,
    };
    let own = repo.list_requests(&filter).await.expect("own page");
    assert_eq!(own.total, 1);
    assert_eq!(own.data[0].created_by, "a-requester");
    let denied = DataAccessRequestFilter {
        approvals: true,
        ..filter
    };
    assert!(repo.list_requests(&denied).await.is_err());
    let approval = DataAccessRequestFilter {
        actor_id: "a-approver".into(),
        ..denied
    };
    let first = repo.list_requests(&approval).await.expect("queue");
    assert_eq!(first.total, 2);
    assert_eq!(first.data.len(), 1);
    let second = repo
        .list_requests(&DataAccessRequestFilter {
            offset: 1,
            ..approval
        })
        .await
        .expect("queue next");
    assert_eq!(second.total, 2);
    assert_ne!(first.data[0].id, second.data[0].id);
}

#[tokio::test]
async fn all_data_requires_real_system_tenant_admin_explicit_permission_and_active_scope() {
    let (pool, repo, _) = setup().await;
    permission(
        &pool,
        "a-requester",
        "custom",
        "data_security:all_data",
        false,
    )
    .await;
    assert!(
        !repo
            .actor_authority("a", "a-requester")
            .await
            .expect("custom")
            .all_data
    );
    permission(
        &pool,
        "a-requester",
        "tenant_admin",
        "data_security:all_data",
        false,
    )
    .await;
    assert!(
        !repo
            .actor_authority("a", "a-requester")
            .await
            .expect("named custom role")
            .all_data
    );
    sqlx::query("UPDATE role SET is_system=1 WHERE id='a-requester-tenant_admin'")
        .execute(&pool)
        .await
        .expect("actual system role");
    assert!(
        repo.actor_authority("a", "a-requester")
            .await
            .expect("system role")
            .all_data
    );
    sqlx::query("DELETE FROM role_permission WHERE role_id='a-requester-tenant_admin'")
        .execute(&pool)
        .await
        .expect("revoke permission");
    assert!(
        !repo
            .actor_authority("a", "a-requester")
            .await
            .expect("explicit permission revoked")
            .all_data
    );
    assert!(
        !repo
            .actor_authority("b", "a-approver")
            .await
            .expect("other tenant")
            .can_approve
    );
    sqlx::query("UPDATE \"user\" SET status='disabled' WHERE id='a-approver'")
        .execute(&pool)
        .await
        .expect("disabled");
    assert!(
        !repo
            .actor_authority("a", "a-approver")
            .await
            .expect("disabled authority")
            .active
    );
}

#[tokio::test]
async fn query_audit_contains_only_bounded_identifiers_and_hashes_with_actor_scoped_pages() {
    let (pool, repo, target) = setup().await;
    let approved = approved(&repo, &target, "audited").await;
    let grant = approved.grant.expect("grant");
    let evidence = DataAccessEvidence {
        query_hash: "a".repeat(64),
        grant_ids: vec![grant.id.clone()],
        metadata_ids: vec![target.metadata_id.clone()],
        columns: vec!["id".into()],
        all_data: false,
    };
    repo.record_access(&RecordDataAccessParams {
        id: "query-observed".into(),
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        outcome: AccessOutcome::Succeeded,
        evidence: evidence.clone(),
    })
    .await
    .expect("query observation");
    let own = repo
        .list_audit(&DataAccessAuditFilter {
            tenant_id: "a".into(),
            actor_id: "a-requester".into(),
            limit: 20,
            offset: 0,
        })
        .await
        .expect("own audit");
    assert_eq!(own.total, 2);
    for audit in &own.data {
        let encoded = audit.evidence.to_string();
        assert!(!encoded.contains("private business purpose"));
        assert!(!encoded.contains("SELECT"));
    }
    let query = own
        .data
        .iter()
        .find(|row| row.id == "query-observed")
        .expect("actual query event");
    assert_eq!(query.evidence["query_hash"], json!("a".repeat(64)));
    assert_eq!(
        repo.list_audit(&DataAccessAuditFilter {
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            limit: 20,
            offset: 0
        })
        .await
        .expect("other audit")
        .total,
        0
    );
    assert_eq!(
        repo.list_audit(&DataAccessAuditFilter {
            tenant_id: "a".into(),
            actor_id: "a-approver".into(),
            limit: 20,
            offset: 0
        })
        .await
        .expect("approval audit")
        .total,
        3
    );
    assert!(
        repo.record_access(&RecordDataAccessParams {
            id: "forged".into(),
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            outcome: AccessOutcome::Succeeded,
            evidence
        })
        .await
        .is_err()
    );
    assert!(
        sqlx::query("UPDATE data_access_audit SET outcome='failed'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_decisions_cannot_double_grant_or_resurrect_after_withdrawal() {
    let (_, repo, target) = setup().await;
    let draft = repo
        .save_request(&save("race", &target))
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
    let approve = decision(
        &pending.request,
        "a-approver",
        DataAccessDecision::Approve,
        "approval",
    );
    let withdraw = decision(
        &pending.request,
        "a-requester",
        DataAccessDecision::Withdraw,
        "withdrawal",
    );
    let (accepted, withdrawn) = tokio::join!(repo.decide(&approve), repo.decide(&withdraw));
    assert_ne!(accepted.is_ok(), withdrawn.is_ok());
    let actual = repo
        .get_request("a", "race")
        .await
        .expect("stored request")
        .expect("request");
    if actual.resource.status == "approved" {
        assert_eq!(
            repo.active_grants("a", "a-requester", Utc::now())
                .await
                .expect("grants")
                .len(),
            1
        );
    } else {
        assert_eq!(actual.resource.status, "withdrawn");
        assert!(
            repo.active_grants("a", "a-requester", Utc::now())
                .await
                .expect("grants")
                .is_empty()
        );
    }
}
