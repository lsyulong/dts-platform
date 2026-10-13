use super::*;
mod fixture;
use crate::SqliteProductResourceRepo;
use dts_backend::ProductResourceRepo;

#[tokio::test]
async fn self_lookup_is_narrow_current_and_does_not_require_directory_permission() {
    let f = Fixture::new().await;
    assert_eq!(
        f.repo.self_organization("tenant", "one").await.unwrap(),
        None
    );
    f.repo
        .transfer(&f.transfer("one", 0, "child"))
        .await
        .unwrap();
    assert_eq!(
        f.repo.self_organization("tenant", "one").await.unwrap(),
        Some("child".into())
    );
    assert!(f.repo.self_organization("other", "one").await.is_err());
    assert!(
        sqlx::query("UPDATE product_resource SET status='inactive' WHERE id='a'")
            .execute(&f.pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='one'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.repo.self_organization("tenant", "one").await,
        Err(BackendError::Forbidden(_))
    ));
    f.repo.remove(&f.remove("one", 1)).await.unwrap();
    sqlx::query("UPDATE \"user\" SET status='active' WHERE id='one'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.repo.self_organization("tenant", "one").await.unwrap(),
        None
    );
}
use fixture::Fixture;

#[tokio::test]
async fn actual_permissions_scope_and_active_identity_are_required() {
    let f = Fixture::new().await;
    let mut p = f.transfer("one", 0, "a");
    p.scope = f.scope("reader");
    assert!(matches!(
        f.repo.transfer(&p).await,
        Err(BackendError::Forbidden(_))
    ));
    p.scope = f.scope("writer");
    assert!(matches!(
        f.repo.transfer(&p).await,
        Err(BackendError::Forbidden(_))
    ));
    p.scope = f.scope("admin");
    p.user_id = "foreign".into();
    assert!(f.repo.transfer(&p).await.is_err());
    p.user_id = "one".into();
    p.target_organization_id = "foreign-org".into();
    assert!(f.repo.transfer(&p).await.is_err());
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='reader'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.repo
            .organization(&ReadOrganizationParams {
                scope: f.scope("reader"),
                organization_id: "a".into()
            })
            .await,
        Err(BackendError::Forbidden(_))
    ));
    let history = f
        .repo
        .history(&OrganizationHistoryFilter {
            scope: f.scope("admin"),
            organization_id: None,
            user_id: None,
            limit: 20,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(history.total, 0);
}

#[tokio::test]
async fn assignment_is_unique_cas_and_does_not_change_roles() {
    let f = Fixture::new().await;
    let before: Vec<String> =
        sqlx::query_scalar("SELECT role_id FROM user_role WHERE user_id='one' ORDER BY role_id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    let a = f.transfer("one", 0, "a");
    let b = f.transfer("one", 0, "b");
    let (one, two) = tokio::join!(f.repo.transfer(&a), f.repo.transfer(&b));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let member = f.membership("one").await.membership.unwrap();
    assert_eq!(member.version, 1);
    assert_eq!(f.history().await.total, 1);
    let after: Vec<String> =
        sqlx::query_scalar("SELECT role_id FROM user_role WHERE user_id='one' ORDER BY role_id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn original_semantic_replay_is_stable_and_actor_private() {
    let f = Fixture::new().await;
    let mut p = f.transfer("one", 0, "a");
    let first = f.repo.transfer(&p).await.unwrap();
    f.repo.transfer(&f.transfer("one", 1, "b")).await.unwrap();
    let replay = f.repo.transfer(&p).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.receipt_id, first.receipt_id);
    assert_eq!(replay.membership.unwrap().version, 1);
    p.reason = "another purpose".into();
    assert!(matches!(
        f.repo.transfer(&p).await,
        Err(BackendError::Conflict(_))
    ));
    p.reason = "岗位调动".into();
    p.scope = f.scope("admin2");
    assert!(matches!(
        f.repo.transfer(&p).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(f.history().await.total, 2);
}

#[tokio::test]
async fn manager_succession_and_departure_are_atomic_real_memberships() {
    let f = Fixture::new().await;
    assert!(
        f.repo
            .set_manager(&f.manager("a", 1, Some("one")))
            .await
            .is_err()
    );
    f.repo.transfer(&f.transfer("one", 0, "a")).await.unwrap();
    f.repo.transfer(&f.transfer("two", 0, "a")).await.unwrap();
    f.repo
        .set_manager(&f.manager("a", 1, Some("one")))
        .await
        .unwrap();
    assert!(f.repo.transfer(&f.transfer("one", 1, "b")).await.is_err());
    let mut p = f.transfer("one", 1, "b");
    p.manager_handover = Some(ManagerHandover {
        expected_organization_version: 2,
        successor_user_id: Some("two".into()),
    });
    let result = f.repo.transfer(&p).await.unwrap();
    assert_eq!(
        result.manager_changes[0].manager.as_ref().unwrap().id,
        "two"
    );
    let a = f.details("a").await;
    assert_eq!(a.manager.unwrap().user.id, "two");
    assert_eq!(a.organization.version, 3);
    assert_eq!(a.member_count, 1);
    assert_eq!(
        f.membership("one")
            .await
            .membership
            .unwrap()
            .organization_id,
        Some("b".into())
    );
}

#[tokio::test]
async fn current_relations_block_delete_and_inactive_but_history_keeps_empty_org_deletable() {
    let f = Fixture::new().await;
    f.repo.transfer(&f.transfer("one", 0, "b")).await.unwrap();
    let resources = SqliteProductResourceRepo::new(f.pool.clone());
    assert!(
        resources
            .delete("tenant", "organization", "b", 1, "admin")
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE product_resource SET status='inactive' WHERE id='b'")
            .execute(&f.pool)
            .await
            .is_err()
    );
    f.repo.remove(&f.remove("one", 1)).await.unwrap();
    resources
        .delete("tenant", "organization", "b", 1, "admin")
        .await
        .unwrap();
    assert_eq!(f.history().await.total, 2);
    assert!(
        resources
            .delete("tenant", "organization", "a", 1, "admin")
            .await
            .is_err()
    ); // actual child FK
}

#[tokio::test]
async fn inactive_member_cleanup_retains_version_and_allows_soft_delete_only_after_removal() {
    let f = Fixture::new().await;
    f.repo.transfer(&f.transfer("one", 0, "a")).await.unwrap();
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='one'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.repo.transfer(&f.transfer("one", 1, "b")).await.is_err());
    assert!(
        sqlx::query("UPDATE \"user\" SET status='deleted' WHERE id='one'")
            .execute(&f.pool)
            .await
            .is_err()
    );
    f.repo.remove(&f.remove("one", 1)).await.unwrap();
    sqlx::query("UPDATE \"user\" SET status='deleted' WHERE id='one'")
        .execute(&f.pool)
        .await
        .unwrap();
    let actual = f.membership("one").await;
    assert_eq!(actual.user.status, "deleted");
    let membership = actual.membership.unwrap();
    assert_eq!(membership.version, 2);
    assert_eq!(membership.organization_id, None);
}

#[tokio::test]
async fn receipt_failure_rolls_back_membership_and_history_is_immutable() {
    let f = Fixture::new().await;
    f.repo.transfer(&f.transfer("one", 0, "a")).await.unwrap();
    sqlx::raw_sql("CREATE TRIGGER fail_member_receipt BEFORE INSERT ON organization_membership_receipt BEGIN SELECT RAISE(ABORT,'fixture_audit_failure'); END;").execute(&f.pool).await.unwrap();
    assert!(f.repo.transfer(&f.transfer("one", 1, "b")).await.is_err());
    assert_eq!(
        f.membership("one")
            .await
            .membership
            .unwrap()
            .organization_id,
        Some("a".into())
    );
    assert_eq!(f.history().await.total, 1);
    assert!(
        sqlx::query("DELETE FROM organization_membership_receipt")
            .execute(&f.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn legacy_json_is_not_membership_and_list_count_uses_the_same_literal_filter() {
    let f = Fixture::with_legacy(true).await;
    let details = f.details("a").await;
    assert!(details.legacy_membership_ignored);
    assert_eq!(details.member_count, 0);
    assert!(details.manager.is_none());
    f.repo.transfer(&f.transfer("one", 0, "b")).await.unwrap();
    f.repo.transfer(&f.transfer("two", 0, "b")).await.unwrap();
    let page = f
        .repo
        .list_members(&OrganizationMemberFilter {
            scope: f.scope("reader"),
            organization_id: "b".into(),
            q: Some("one".into()),
            limit: 1,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].user.id, "one");
    assert!(
        sqlx::query(r#"UPDATE product_resource SET properties='{"member_ids":[]}' WHERE id='b'"#)
            .execute(&f.pool)
            .await
            .is_err()
    );
}
