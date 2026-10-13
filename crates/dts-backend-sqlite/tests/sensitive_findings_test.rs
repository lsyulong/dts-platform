mod product_job_support;
use dts_backend::repo::{
    metadata_catalog::*, product_job::*, product_resource::*, sensitive_findings::*,
};
use dts_backend_sqlite::{
    SqliteProductJobRepo, SqliteProductResourceRepo, SqliteSensitiveFindingRepo,
};
use product_job_support::*;
use serde_json::{Value, json};
use sqlx::SqlitePool;

async fn fixture() -> (
    SqlitePool,
    SqliteProductJobRepo,
    SqliteSensitiveFindingRepo,
    SensitiveCommitParams,
) {
    let (pool, jobs) = setup().await;
    sqlx::raw_sql(include_str!("../migrations/018_sensitive_findings.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO role(id,tenant_id,name) VALUES('recognition-role','a','Recognition tests')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for permission in [
        "sensitive_data:read",
        "sensitive_data:write",
        "sensitive_data:execute",
    ] {
        sqlx::query(
            "INSERT INTO role_permission(role_id,permission) VALUES('recognition-role',?1)",
        )
        .bind(permission)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES('a-owner','recognition-role')")
        .execute(&pool)
        .await
        .unwrap();
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "customers".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![MetadataColumn {
            name: "email".into(),
            data_type: "VARCHAR".into(),
            nullable: true,
            ordinal_position: 1,
        }],
    };
    let meta = table.properties("a").unwrap();
    let fingerprint = table.fingerprint().unwrap();
    let binding = MetadataEngineBinding {
        engine: "swanlake".into(),
        tenant_id: "a".into(),
        object_version: 1,
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "customers".into(),
        source_fingerprint: fingerprint.clone(),
    };
    let class =
        json!({"id":"class","version":1,"properties":{"rank":2,"handling_policy":"人工复核"}});
    let rule = json!({"id":"rule","version":1,"properties":{"classification_id":"class","pattern":"@","sample_limit":3,"match_target":"sample_value","source_types":["text"],"publication":{"format_version":1,"classification":class}}});
    let metadata = json!({"id":"metadata","version":1,"properties":meta,"binding":binding,"columns":table.columns});
    let task_props = json!({"rule_id":"rule","object_id":"metadata","sample_limit":3,"publication":{"format_version":1,"rule":rule,"classification":class,"metadata":metadata}});
    let resources = SqliteProductResourceRepo::new(pool.clone());
    for (kind, id, props) in [
        ("data_classification", "class", class["properties"].clone()),
        ("recognition_rule", "rule", rule["properties"].clone()),
        ("metadata_object", "metadata", meta),
        ("recognition_task", "task", task_props.clone()),
    ] {
        resources
            .create(&CreateResourceParams {
                id: id.into(),
                tenant_id: "a".into(),
                kind: kind.into(),
                actor_id: "a-owner".into(),
                fields: ResourceWrite {
                    code: id.into(),
                    name: id.into(),
                    description: None,
                    parent_id: None,
                    status: "published".into(),
                    properties: props,
                    references: vec![],
                },
            })
            .await
            .unwrap();
    }
    // Test-only registry observation. No server test pretends to contact SwanLake.
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at) VALUES('a','swanlake','lake','main','customers','metadata',1,'BASE TABLE',?1,?2,'present','test-only-observation',strftime('%Y-%m-%dT%H:%M:%fZ','now'))").bind(json!(table.columns).to_string()).bind(fingerprint).execute(&pool).await.unwrap();
    let snapshot = json!({"format_version":1,"executor":"sensitive_scan_v1","tenant_id":"a","source_kind":"recognition_task","source_id":"task","source_version":1,"properties":task_props,"task":{"id":"task","version":1,"properties":task_props},"rule":rule,"classification":class,"metadata":metadata});
    let mut requested = request("scan-job");
    requested.source_kind = "recognition_task".into();
    requested.source_id = "task".into();
    requested.action = "execute_recognition".into();
    requested.definition_snapshot = snapshot.clone();
    requested.nodes = vec![ProductJobNodeDefinition {
        id: "recognition".into(),
        name: "recognition".into(),
        definition: json!({"format_version":1,"type":"sensitive_scan","definition_hash":snapshot_hash(&snapshot).unwrap()}),
        depends_on: vec![],
    }];
    jobs.enqueue(&requested).await.unwrap();
    let mut claim = claim_params("scanner");
    claim.source_kinds = vec!["recognition_task".into()];
    let job = jobs.claim(&claim).await.unwrap().unwrap();
    jobs.start_node(&start(&job, "recognition")).await.unwrap();
    let params = SensitiveCommitParams {
        tenant_id: "a".into(),
        actor_id: "a-owner".into(),
        job_id: job.id.clone(),
        node_id: "recognition".into(),
        lease: lease(&job),
        snapshot: snapshot.clone(),
        result: SensitiveScanResult {
            evaluation_scope: "sample".into(),
            sample_limit: 3,
            sampled_rows: 3,
            columns: vec![SensitiveColumnEvidence {
                column_name: "email".into(),
                data_type: "VARCHAR".into(),
                checked_values: 2,
                matched_values: 1,
                null_values: 1,
            }],
            definition_hash: snapshot_hash(&snapshot).unwrap(),
            producer_committed: false,
            finding_ids: vec![],
        },
    };
    (
        pool.clone(),
        jobs,
        SqliteSensitiveFindingRepo::new(pool),
        params,
    )
}
fn review(id: &str, confirm: bool) -> SensitiveReviewParams {
    SensitiveReviewParams {
        tenant_id: "a".into(),
        actor_id: "a-owner".into(),
        finding_id: id.into(),
        expected_version: 1,
        confirm,
        reason: "对真实匹配计数完成人工复核".into(),
    }
}
async fn count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sensitive_finding")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn actual_producer_commits_job_node_findings_and_review_atomically() {
    let (pool, jobs, repo, params) = fixture().await;
    let result = repo.commit_scan(&params).await.unwrap();
    assert!(result.producer_committed);
    assert_eq!(result.finding_ids.len(), 1);
    let job = jobs.get(&lookup("scan-job")).await.unwrap().unwrap();
    assert_eq!(job.status, ProductJobStatus::Succeeded);
    assert_eq!(job.nodes[0].status, ProductJobNodeStatus::Succeeded);
    assert_eq!(job.result, Some(json!(result)));
    assert!(repo.commit_scan(&params).await.is_err());
    assert_eq!(count(&pool).await, 1);
    let found = repo
        .get("a", "a-owner", &result.finding_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.evidence.matched_values, 1);
    assert_eq!(found.status, "pending");
    let confirmed = repo.review(&review(&found.id, true)).await.unwrap();
    assert_eq!(confirmed.status, "confirmed");
    assert_eq!(confirmed.version, 2);
    assert!(repo.review(&review(&found.id, false)).await.is_err());
    let projection: String =
        sqlx::query_scalar("SELECT properties FROM product_resource WHERE id=?1")
            .bind(&found.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let projection: Value = serde_json::from_str(&projection).unwrap();
    assert!(projection["evidence"].get("matched_values").is_none());
    assert_eq!(projection["confirmed"], true);
    let audits: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sensitive_finding_audit WHERE finding_id=?1")
            .bind(&found.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 2);
}
#[tokio::test]
async fn expired_fence_cancel_and_stale_registry_rollback_all_findings() {
    for failure in ["fence", "expired", "cancel", "metadata", "permission"] {
        let (pool, jobs, repo, mut params) = fixture().await;
        match failure {
            "fence" => params.lease.fence += 1,
            "expired" => expire(&pool, "scan-job").await,
            "cancel" => {
                sqlx::query("UPDATE product_job SET cancel_requested=1 WHERE id='scan-job'")
                    .execute(&pool)
                    .await
                    .unwrap();
            },
            "metadata" => {
                sqlx::query("UPDATE metadata_catalog SET discovery_status='missing' WHERE object_id='metadata'").execute(&pool).await.unwrap();
            },
            _ => {
                sqlx::query(
                    "DELETE FROM role_permission WHERE permission='sensitive_data:execute'",
                )
                .execute(&pool)
                .await
                .unwrap();
            },
        }
        assert!(repo.commit_scan(&params).await.is_err(), "{failure}");
        assert_eq!(count(&pool).await, 0);
        assert_eq!(
            jobs.get(&lookup("scan-job")).await.unwrap().unwrap().status,
            ProductJobStatus::Running
        );
    }
}
#[tokio::test]
async fn empty_matches_are_success_without_manufactured_findings() {
    let (pool, jobs, repo, mut params) = fixture().await;
    params.result.columns[0].matched_values = 0;
    let result = repo.commit_scan(&params).await.unwrap();
    assert!(result.producer_committed);
    assert!(result.finding_ids.is_empty());
    assert_eq!(count(&pool).await, 0);
    assert_eq!(
        jobs.get(&lookup("scan-job")).await.unwrap().unwrap().status,
        ProductJobStatus::Succeeded
    );
}
#[tokio::test]
async fn changed_definition_prevents_confirmation_but_reasoned_dismissal_works() {
    let (pool, _, repo, params) = fixture().await;
    let result = repo.commit_scan(&params).await.unwrap();
    let id = &result.finding_ids[0];
    sqlx::query("UPDATE product_resource SET status='inactive',version=version+1 WHERE kind='recognition_task' AND id='task'").execute(&pool).await.unwrap();
    assert!(repo.review(&review(id, true)).await.is_err());
    let dismissed = repo.review(&review(id, false)).await.unwrap();
    assert_eq!(dismissed.status, "dismissed");
    assert!(repo.get("b", "a-owner", id).await.is_err());
}
#[tokio::test]
async fn projection_conflict_rolls_back_typed_review_and_audit() {
    let (pool, _, repo, params) = fixture().await;
    let result = repo.commit_scan(&params).await.unwrap();
    let id = &result.finding_ids[0];
    sqlx::query("UPDATE product_resource SET version=version+1 WHERE id=?1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.review(&review(id, true)).await.is_err());
    let found = repo.get("a", "a-owner", id).await.unwrap().unwrap();
    assert_eq!(found.status, "pending");
    assert_eq!(found.version, 1);
    let audits: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sensitive_finding_audit WHERE finding_id=?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 1);
}
