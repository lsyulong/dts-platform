// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

mod product_job_support;
use chrono::{Duration, Utc};
use dts_backend::{
    BackendError,
    repo::{bi_share::*, product_job::*, product_resource::*},
};
use dts_backend_sqlite::{SqliteBiShareRepo, SqliteProductResourceRepo};
use product_job_support::*;
use serde_json::json;
use sqlx::SqlitePool;

async fn fixture(finish: bool) -> (SqlitePool, SqliteBiShareRepo) {
    let (pool, jobs) = setup().await;
    sqlx::raw_sql(include_str!("../migrations/017_bi_shares.sql"))
        .execute(&pool)
        .await
        .unwrap();
    for tenant in ["a", "b"] {
        let role = format!("{tenant}-share-role");
        sqlx::query("INSERT INTO role(id,tenant_id,name) VALUES(?1,?2,?1)")
            .bind(&role)
            .bind(tenant)
            .execute(&pool)
            .await
            .unwrap();
        for permission in ["bi:read", "bi:write"] {
            sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES(?1,?2)")
                .bind(&role)
                .bind(permission)
                .execute(&pool)
                .await
                .unwrap();
        }
        for actor in [format!("{tenant}-owner"), format!("{tenant}-other")] {
            sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES(?1,?2)")
                .bind(actor)
                .bind(&role)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    let properties = json!({"fixture":"BI repository control-plane observation"});
    SqliteProductResourceRepo::new(pool.clone())
        .create(&CreateResourceParams {
            id: "a-bi".into(),
            tenant_id: "a".into(),
            kind: "bi_report".into(),
            actor_id: "a-owner".into(),
            fields: ResourceWrite {
                code: "bi".into(),
                name: "BI repository contract".into(),
                description: None,
                parent_id: None,
                status: "published".into(),
                properties: properties.clone(),
                references: vec![],
            },
        })
        .await
        .unwrap();
    let mut requested = request("bi-observation");
    requested.source_kind = "bi_report".into();
    requested.source_id = "a-bi".into();
    requested.action = "execute_bi".into();
    requested.definition_snapshot = json!({"executor":"bi_aggregate_v1","properties":properties});
    jobs.enqueue(&requested).await.unwrap();
    if finish {
        let mut claim = claim_params("bi-worker");
        claim.source_kinds = vec!["bi_report".into()];
        let job = jobs.claim(&claim).await.unwrap().unwrap();
        jobs.start_node(&start(&job, "one")).await.unwrap();
        // Explicit test-only observed counter, through actual node/job state
        // transitions. Data grants/masking are tested in the server layer.
        jobs.finish_node(&node_finish(&job, "one", ProductJobOutcome::Succeeded))
            .await
            .unwrap();
        jobs.finish(&product_job_support::finish(
            &job,
            ProductJobOutcome::Succeeded,
        ))
        .await
        .unwrap();
    }
    let shares = SqliteBiShareRepo::new(pool.clone());
    (pool, shares)
}

fn create(id: &str) -> CreateBiShareParams {
    CreateBiShareParams {
        id: id.into(),
        tenant_id: "a".into(),
        owner_id: "a-owner".into(),
        recipient_id: "a-other".into(),
        report_id: "a-bi".into(),
        report_version: 1,
        job_id: "bi-observation".into(),
        node_id: "one".into(),
        expires_at: Utc::now() + Duration::hours(1),
    }
}
fn share_lookup(actor: &str, id: &str) -> BiShareLookup {
    BiShareLookup {
        tenant_id: "a".into(),
        actor_id: actor.into(),
        share_id: id.into(),
    }
}

#[tokio::test]
async fn share_binds_real_participants_result_version_and_atomic_revoke() {
    let (pool, repo) = fixture(true).await;
    let created = repo.create(&create("share")).await.unwrap();
    assert_eq!(created.status, BiShareStatus::Active);
    assert_eq!(created.version, 1);
    assert!(!created.is_expired);
    assert_eq!(
        repo.open(&share_lookup("a-other", "share"))
            .await
            .unwrap()
            .id,
        "share"
    );
    assert!(repo.get(&share_lookup("b-other", "share")).await.is_err());
    assert!(
        repo.revoke(&RevokeBiShareParams {
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            share_id: "share".into(),
            expected_version: 1
        })
        .await
        .is_err()
    );
    let revoked = repo
        .revoke(&RevokeBiShareParams {
            tenant_id: "a".into(),
            actor_id: "a-owner".into(),
            share_id: "share".into(),
            expected_version: 1,
        })
        .await
        .unwrap();
    assert_eq!(revoked.status, BiShareStatus::Revoked);
    assert_eq!(revoked.version, 2);
    assert!(revoked.revoked_at.is_some());
    assert!(repo.open(&share_lookup("a-other", "share")).await.is_err());
    assert!(
        repo.revoke(&RevokeBiShareParams {
            tenant_id: "a".into(),
            actor_id: "a-owner".into(),
            share_id: "share".into(),
            expected_version: 1
        })
        .await
        .is_err()
    );
    let audits: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bi_share_audit WHERE share_id='share'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 3);
    assert!(
        sqlx::query("DELETE FROM product_job WHERE id='bi-observation'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn queued_nodes_wrong_version_and_cross_tenant_recipients_do_not_create_shares_or_audits() {
    let (pool, repo) = fixture(false).await;
    assert!(repo.create(&create("queued")).await.is_err());
    let mut invalid = create("foreign");
    invalid.recipient_id = "b-other".into();
    assert!(repo.create(&invalid).await.is_err());
    invalid = create("version");
    invalid.report_version = 2;
    assert!(repo.create(&invalid).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bi_share")
        .fetch_one(&pool)
        .await
        .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bi_share_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(audits, 0);
}

#[tokio::test]
async fn db_clock_expiry_current_publication_and_current_owner_role_are_rechecked() {
    let (pool, repo) = fixture(true).await;
    repo.create(&create("expired")).await.unwrap();
    // Test-only clock fixture; no wall-clock sleep or production expiry bypass.
    sqlx::query("UPDATE bi_share SET created_at='2020-01-01T00:00:00.000Z',expires_at='2020-01-02T00:00:00.000Z' WHERE id='expired'").execute(&pool).await.unwrap();
    assert!(
        repo.get(&share_lookup("a-other", "expired"))
            .await
            .unwrap()
            .unwrap()
            .is_expired
    );
    assert!(
        repo.open(&share_lookup("a-other", "expired"))
            .await
            .is_err()
    );
    repo.create(&create("current")).await.unwrap();
    sqlx::query(
        "DELETE FROM role_permission WHERE role_id='a-share-role' AND permission='bi:write'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        repo.open(&share_lookup("a-other", "current"))
            .await
            .is_err()
    );
    sqlx::query(
        "INSERT INTO role_permission(role_id,permission) VALUES('a-share-role','bi:write')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE product_resource SET status='inactive',version=version+1 WHERE id='a-bi'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo.open(&share_lookup("a-other", "current"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn expiry_bounds_recipient_status_and_pagination_fail_closed() {
    let (pool, repo) = fixture(true).await;
    let mut invalid = create("past");
    invalid.expires_at = Utc::now() - Duration::seconds(1);
    assert!(matches!(
        repo.create(&invalid).await,
        Err(BackendError::Conflict(_))
    ));
    invalid = create("long");
    invalid.expires_at = Utc::now() + Duration::days(31);
    assert!(repo.create(&invalid).await.is_err());
    repo.create(&create("valid")).await.unwrap();
    let page = repo
        .list(&BiShareFilter {
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            report_id: Some("a-bi".into()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data.len(), 1);
    assert!(
        repo.list(&BiShareFilter {
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            report_id: None,
            limit: 0,
            offset: 0
        })
        .await
        .is_err()
    );
    sqlx::query("UPDATE \"user\" SET status='disabled' WHERE id='a-other'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.open(&share_lookup("a-owner", "valid")).await.is_err());
}
