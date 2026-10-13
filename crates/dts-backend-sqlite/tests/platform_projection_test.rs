mod data_access_support;
mod master_data_support;
mod product_job_support;
use dts_backend::{
    BackendError,
    repo::{
        data_access::*, master_data::*, message_delivery::*, platform_projection::*,
        product_job::*, product_resource::*,
    },
};
use dts_backend_sqlite::{
    SqliteMessageDeliveryRepo, SqlitePlatformProjectionRepo, SqliteProductResourceRepo,
};
use serde_json::json;
use sqlx::SqlitePool;

async fn permission(pool: &SqlitePool, actor: &str, values: &[&str]) {
    let role = format!("projection-{actor}");
    sqlx::query("INSERT INTO role(id,tenant_id,name) VALUES(?1,'a',?1)")
        .bind(&role)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES(?1,?2)")
        .bind(actor)
        .bind(&role)
        .execute(pool)
        .await
        .unwrap();
    for value in values {
        sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES(?1,?2)")
            .bind(&role)
            .bind(value)
            .execute(pool)
            .await
            .unwrap();
    }
}
async fn request(repo: &SqlitePlatformProjectionRepo, actor: &str) -> ProjectionRequest {
    ProjectionRequest {
        tenant_id: "a".into(),
        actor_id: actor.into(),
        expected_permissions: repo.authority("a", actor).await.unwrap().permissions,
        kinds: vec![
            ProjectionKind {
                kind: "sql_document".into(),
                scope: "development".into(),
                path: "/development/sql".into(),
            },
            ProjectionKind {
                kind: "master_data_record".into(),
                scope: "master_data".into(),
                path: "/master-data/records".into(),
            },
        ],
        limit: 20,
        offset: 0,
    }
}

#[tokio::test]
async fn queued_jobs_are_real_owned_rows_and_permissions_are_rechecked() {
    let (pool, jobs) = product_job_support::setup().await;
    permission(
        &pool,
        "a-owner",
        &["development:read", "development:execute"],
    )
    .await;
    jobs.enqueue(&product_job_support::request("owned"))
        .await
        .unwrap();
    let mut other = product_job_support::request("other");
    other.actor_id = "a-other".into();
    assert!(jobs.enqueue(&other).await.is_err());
    // Another actual source avoids the repository's one-active-run-per-source invariant.
    other.source_id = "a-workflow".into();
    other.source_kind = "development_workflow".into();
    jobs.enqueue(&other).await.unwrap();
    let repo = SqlitePlatformProjectionRepo::new(pool.clone());
    let p = request(&repo, "a-owner").await;
    let page = repo.work_source(&p, WorkSource::QueuedJobs).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].id, "owned");
    assert_eq!(page.data[0].status, "queued");
    sqlx::query("DELETE FROM role_permission WHERE role_id='projection-a-owner' AND permission='development:execute'").execute(&pool).await.unwrap();
    assert!(matches!(
        repo.work_source(&p, WorkSource::QueuedJobs).await,
        Err(BackendError::Forbidden(_))
    ));
    let fresh = request(&repo, "a-owner").await;
    assert!(
        repo.work_source(&fresh, WorkSource::QueuedJobs)
            .await
            .is_err()
    );
    let forged = ProjectionRequest {
        tenant_id: "b".into(),
        ..p
    };
    assert!(
        repo.work_source(&forged, WorkSource::QueuedJobs)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn inbox_uses_actual_recipient_and_read_receipts_without_message_body() {
    let (pool, _) = product_job_support::setup().await;
    let resource = SqliteProductResourceRepo::new(pool.clone())
        .create(&CreateResourceParams {
            id: "notice".into(),
            tenant_id: "a".into(),
            actor_id: "a-other".into(),
            kind: "system_message".into(),
            fields: ResourceWrite {
                code: "notice".into(),
                name: "Notice".into(),
                status: "draft".into(),
                description: None,
                parent_id: None,
                properties: json!({"channel":"in_app","recipient_ids":["a-owner","a-other"]}),
                references: vec![],
            },
        })
        .await
        .unwrap();
    let delivery = SqliteMessageDeliveryRepo::new(pool.clone());
    let sent = delivery
        .deliver_in_app(&DeliverInAppParams {
            tenant_id: "a".into(),
            message_id: resource.id,
            expected_version: resource.version,
            actor_id: "a-other".into(),
            idempotency_key: "notice-send".into(),
            subject: "Actual notice".into(),
            body: "Private body must not be projected".into(),
            recipient_ids: vec!["a-owner".into(), "a-other".into()],
        })
        .await
        .unwrap();
    let repo = SqlitePlatformProjectionRepo::new(pool);
    let p = request(&repo, "a-owner").await;
    let page = repo
        .work_source(&p, WorkSource::UnreadMessages)
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].title, "Actual notice");
    assert!(
        !serde_json::to_string(&page)
            .unwrap()
            .contains("Private body")
    );
    let mine = sent
        .data
        .iter()
        .find(|r| r.recipient_id == "a-owner")
        .unwrap();
    delivery.mark_read("a", "a-owner", &mine.id).await.unwrap();
    assert_eq!(
        repo.work_source(&p, WorkSource::UnreadMessages)
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn actual_master_pending_requires_distinct_approver_and_pending_identity() {
    let (pool, master) = master_data_support::setup().await;
    permission(
        &pool,
        "a-owner",
        &["master_data:read", "master_data:approve"],
    )
    .await;
    permission(&pool, "a-approver", &["master_data:read"]).await;
    let pending = master_data_support::submitted(&master, "actual-record").await;
    let repo = SqlitePlatformProjectionRepo::new(pool);
    assert_eq!(
        repo.work_source(
            &request(&repo, "a-owner").await,
            WorkSource::MasterDataApprovals
        )
        .await
        .unwrap()
        .total,
        0
    );
    let p = request(&repo, "a-approver").await;
    let actual = repo
        .work_source(&p, WorkSource::MasterDataApprovals)
        .await
        .unwrap();
    assert_eq!(actual.total, 1);
    assert_eq!(actual.data[0].source_version, Some(pending.version));
    master
        .transition_record(&MasterRecordActionParams {
            scope: master_data_support::scope(&pending, "a-approver"),
            action: MasterRecordAction::Approve,
            reason: "verified".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        repo.work_source(&p, WorkSource::MasterDataApprovals)
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn actual_access_pending_excludes_requester_and_disappears_after_decision() {
    let (pool, access, target) = data_access_support::setup().await;
    permission(
        &pool,
        "a-requester",
        &["data_security:read", "data_security:approve"],
    )
    .await;
    permission(&pool, "a-approver", &["data_security:read"]).await;
    let draft = access
        .save_request(&data_access_support::save("actual-request", &target))
        .await
        .unwrap();
    let pending = access
        .decide(&data_access_support::decision(
            &draft,
            "a-requester",
            DataAccessDecision::Submit,
            "submit",
        ))
        .await
        .unwrap()
        .request;
    let repo = SqlitePlatformProjectionRepo::new(pool);
    assert_eq!(
        repo.work_source(
            &request(&repo, "a-requester").await,
            WorkSource::DataAccessApprovals
        )
        .await
        .unwrap()
        .total,
        0
    );
    let p = request(&repo, "a-approver").await;
    assert_eq!(
        repo.work_source(&p, WorkSource::DataAccessApprovals)
            .await
            .unwrap()
            .total,
        1
    );
    access
        .decide(&data_access_support::decision(
            &pending,
            "a-approver",
            DataAccessDecision::Reject,
            "reject",
        ))
        .await
        .unwrap();
    assert_eq!(
        repo.work_source(&p, WorkSource::DataAccessApprovals)
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn definitions_and_executions_have_different_actual_counts_and_half_open_window() {
    let (pool, jobs) = product_job_support::setup().await;
    permission(
        &pool,
        "a-owner",
        &["development:read", "development:execute"],
    )
    .await;
    jobs.enqueue(&product_job_support::request("run"))
        .await
        .unwrap();
    sqlx::query("UPDATE product_job SET created_at='2026-10-09T00:00:00Z' WHERE id='run'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE product_resource SET created_at='2026-10-08T23:00:00Z' WHERE id='a-sql'")
        .execute(&pool)
        .await
        .unwrap();
    let repo = SqlitePlatformProjectionRepo::new(pool);
    let p = StatisticsRequest {
        actor: request(&repo, "a-owner").await,
        start: "2026-10-08T00:00:00Z".parse().unwrap(),
        end: "2026-10-09T00:00:00Z".parse().unwrap(),
    };
    assert_eq!(
        repo.statistics_source(&p, StatisticsSource::ProductJobs)
            .await
            .unwrap()
            .total,
        0
    );
    let broad = StatisticsRequest {
        start: "2026-10-08T00:00:00Z".parse().unwrap(),
        end: "2026-10-10T00:00:00Z".parse().unwrap(),
        ..p
    };
    let definitions = repo
        .statistics_source(&broad, StatisticsSource::ProductDefinitions)
        .await
        .unwrap();
    assert_eq!(definitions.total, 1);
    let jobs = repo
        .statistics_source(&broad, StatisticsSource::ProductJobs)
        .await
        .unwrap();
    assert_eq!(jobs.total, 1);
    assert!(jobs.data.iter().all(|v| v.dimension.ends_with(":queued")));
}

#[tokio::test]
async fn missing_producer_table_returns_error_never_fabricated_zero() {
    let (pool, _) = product_job_support::setup().await;
    sqlx::query("DROP TABLE message_delivery")
        .execute(&pool)
        .await
        .unwrap();
    let repo = SqlitePlatformProjectionRepo::new(pool);
    let p = request(&repo, "a-owner").await;
    assert!(matches!(
        repo.work_source(&p, WorkSource::UnreadMessages).await,
        Err(BackendError::Query(_))
    ));
}
