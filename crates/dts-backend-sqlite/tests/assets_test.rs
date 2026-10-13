use chrono::{Duration, Utc};
use dts_backend::{
    repo::assets::{validation, *},
    *,
};
use serde_json::json;

mod assets_support;
use assets_support::*;

#[tokio::test]
async fn actual_source_owner_domain_and_actor_fail_atomically() {
    let (pool, repo) = setup().await;
    let mut invalid = entry("bad_source", "metadata_object", "missing");
    assert!(repo.save(&invalid).await.is_err());
    invalid = entry("bad_owner", "metadata_object", "metadata-a");
    invalid.fields.properties["owner_id"] = json!("b-approver");
    invalid.fields.references =
        validation::references("asset_entry", &invalid.fields.properties).expect("refs");
    assert!(repo.save(&invalid).await.is_err());
    invalid = entry("bad_domain", "metadata_object", "metadata-a");
    invalid.fields.properties["domain_id"] = json!("absent");
    invalid.fields.references =
        validation::references("asset_entry", &invalid.fields.properties).expect("refs");
    assert!(repo.save(&invalid).await.is_err());
    invalid = entry("bad_actor", "metadata_object", "metadata-a");
    invalid.actor_id = "b-requester".into();
    assert!(repo.save(&invalid).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE kind='asset_entry'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 0);
    let mut forged = entry("forged", "metadata_object", "metadata-a");
    forged.fields.properties["proof"] = json!({"type":"metadata_catalog"});
    assert!(repo.save(&forged).await.is_err());
}

#[tokio::test]
async fn publication_freezes_real_version_and_is_immutable() {
    let (pool, repo) = setup().await;
    let draft = repo
        .save(&entry("catalog", "metadata_object", "metadata-a"))
        .await
        .expect("draft");
    assert!(
        transition(&repo, &draft, AssetTransition::Publish)
            .await
            .is_err()
    );
    let registered = transition(&repo, &draft, AssetTransition::Register)
        .await
        .expect("registered");
    let published = transition(&repo, &registered.resource, AssetTransition::Publish)
        .await
        .expect("published");
    assert_eq!(published.resource.status, "published");
    assert!(published.source_valid);
    assert!(!published.stale);
    let receipt = published.publication.expect("receipt");
    assert_eq!(receipt.source_id, "metadata-a");
    assert_eq!(receipt.source_version, 1);
    assert_eq!(receipt.asset_version, 3);
    assert_eq!(receipt.proof["type"], "metadata_catalog");
    assert_eq!(receipt.publication_scope, "catalog_definition");
    assert!(!receipt.physical_published);
    let audit: Vec<String> = sqlx::query_scalar("SELECT operation||':'||json_extract(summary,'$.action') FROM product_resource_audit WHERE resource_id='catalog' ORDER BY json_extract(summary,'$.after.version')")
        .fetch_all(&pool).await.expect("real transition audit");
    assert_eq!(
        audit,
        ["create:create", "update:register", "update:publish"]
    );
    assert!(
        transition(&repo, &registered.resource, AssetTransition::Publish)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE asset_publication SET source_version=2")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM asset_publication")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM product_resource WHERE id='metadata-a'")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn changed_source_is_stale_and_withdraw_remains_possible() {
    let (pool, repo) = setup().await;
    let published = publish(&repo, "catalog").await;
    sqlx::query("UPDATE metadata_catalog SET object_version=2 WHERE object_id='metadata-a'")
        .execute(&pool)
        .await
        .expect("collector newer");
    sqlx::query("UPDATE product_resource SET version=2 WHERE id='metadata-a'")
        .execute(&pool)
        .await
        .expect("newer resource");
    let view = repo
        .get_asset(&read("catalog"))
        .await
        .expect("read")
        .expect("view");
    assert!(view.stale);
    assert!(!view.source_valid);
    let withdrawn = transition(&repo, &published.resource, AssetTransition::Withdraw)
        .await
        .expect("withdraw");
    assert_eq!(withdrawn.resource.status, "withdrawn");
    let mut edit = entry("catalog", "metadata_object", "metadata-a");
    edit.expected_version = Some(withdrawn.resource.version);
    let draft = repo.save(&edit).await.expect("edit withdrawn into draft");
    assert_eq!(draft.status, "draft");
    let registered = transition(&repo, &draft, AssetTransition::Register)
        .await
        .expect("register new");
    let next = transition(&repo, &registered.resource, AssetTransition::Publish)
        .await
        .expect("new publication");
    assert!(next.source_valid);
    assert_eq!(next.publication.expect("new receipt").source_version, 2);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asset_publication")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 2);
}

#[tokio::test]
async fn definition_validation_version_and_actual_permissions_are_required() {
    let (pool, repo) = setup().await;
    let draft = repo
        .save(&entry("catalog", "metadata_object", "metadata-a"))
        .await
        .expect("draft");
    let wrong = TransitionAssetParams {
        scope: scope(&draft),
        action: AssetTransition::Register,
        validated_source: Some(AssetSourceCheck {
            kind: "metadata_object".into(),
            id: "metadata-a".into(),
            version: 2,
            definition_hash: "fake".into(),
        }),
    };
    assert!(repo.transition(&wrong).await.is_err());
    sqlx::query("DELETE FROM role_permission WHERE permission='assets:execute'")
        .execute(&pool)
        .await
        .expect("revoke execute");
    let registered = transition(&repo, &draft, AssetTransition::Register)
        .await
        .expect("write may register");
    assert!(
        transition(&repo, &registered.resource, AssetTransition::Publish)
            .await
            .is_err()
    );
    assert!(
        repo.get_asset(&read("catalog"))
            .await
            .expect("reader")
            .is_some()
    );
    sqlx::query("DELETE FROM role_permission WHERE permission='metadata:read'")
        .execute(&pool)
        .await
        .expect("revoke source read");
    assert!(repo.get_asset(&read("catalog")).await.is_err());
    let page = repo
        .list_assets(&AssetListParams {
            actor_id: "a-requester".into(),
            filter: ProductResourceFilter {
                tenant_id: "a".into(),
                kind: "asset_entry".into(),
                q: None,
                status: None,
                created_by: None,
                limit: 10,
                offset: 0,
            },
        })
        .await
        .expect("filtered directory");
    assert_eq!(page.total, 0);
    assert!(page.data.is_empty());
}

#[tokio::test]
async fn real_source_references_become_edges_only_for_actual_readable_assets() {
    let (pool, repo) = setup().await;
    source(
        &pool,
        "atomic",
        "atomic_metric",
        json!({"model_id":"actual_model","field_name":"amount","aggregation":"sum"}),
    )
    .await;
    source(
        &pool,
        "derived",
        "derived_metric",
        json!({"atomic_metric_id":"atomic","filter_expression":"amount > 0"}),
    )
    .await;
    sqlx::query("INSERT INTO product_resource_links(tenant_id,source_kind,source_id,field,target_kind,target_id)
        VALUES('a','derived_metric','derived','atomic_metric_id','atomic_metric','atomic')").execute(&pool).await.expect("true source reference");
    for (id, kind, src) in [
        ("a_atomic", "atomic_metric", "atomic"),
        ("a_derived", "derived_metric", "derived"),
    ] {
        repo.save(&entry(id, kind, src))
            .await
            .expect("actual asset");
    }
    let map = repo
        .save(&save(
            "map",
            "asset_map",
            json!({"asset_ids":["a_atomic","a_derived"]}),
        ))
        .await
        .expect("map");
    let snapshot = repo
        .map_snapshot(&AssetMapSnapshotParams {
            scope: scope(&map),
            max_nodes: 10,
            max_edges: 10,
        })
        .await
        .expect("snapshot");
    assert_eq!(snapshot.nodes.len(), 2);
    assert_eq!(snapshot.edges.len(), 1);
    assert_eq!(snapshot.edges[0].source_asset_id, "a_derived");
    assert_eq!(snapshot.edges[0].target_asset_id, "a_atomic");
    assert_eq!(snapshot.edges[0].evidence["field"], "atomic_metric_id");
    sqlx::query("DELETE FROM role_permission WHERE permission='metrics:read'")
        .execute(&pool)
        .await
        .expect("revoke source read");
    let snapshot = repo
        .map_snapshot(&AssetMapSnapshotParams {
            scope: scope(&map),
            max_nodes: 10,
            max_edges: 10,
        })
        .await
        .expect("filtered snapshot");
    assert!(snapshot.nodes.is_empty());
    assert!(snapshot.edges.is_empty());
    assert!(snapshot.invalid_nodes.is_empty());
}

#[tokio::test]
async fn usage_counts_only_actual_scoped_recorded_query_events() {
    let (pool, repo) = setup().await;
    publish(&repo, "catalog").await;
    let params = usage("catalog");
    let recorded = params.request.period_start + Duration::minutes(5);
    audit(
        &pool,
        "ok",
        "a-requester",
        "metadata-a",
        "succeeded",
        recorded,
    )
    .await;
    audit(&pool, "failed", "a-other", "metadata-a", "failed", recorded).await;
    audit(
        &pool,
        "denied",
        "a-requester",
        "metadata-a",
        "denied",
        recorded,
    )
    .await;
    audit(
        &pool,
        "outside",
        "a-requester",
        "metadata-a",
        "succeeded",
        params.request.period_start - Duration::seconds(1),
    )
    .await;
    audit(
        &pool,
        "unrelated",
        "a-requester",
        "other-metadata",
        "succeeded",
        recorded,
    )
    .await;
    let result = repo.analyze_usage(&params).await.expect("usage");
    assert!(result.complete);
    assert_eq!(result.instrumentation, "data_access_audit");
    assert_eq!(result.scope, "recorded_controlled_query_events_only");
    assert_eq!(result.matched_recorded_events, Some(3));
    assert_eq!(result.counted_recorded_events, 3);
    assert_eq!(
        (
            result.succeeded,
            result.failed,
            result.denied,
            result.unique_actors
        ),
        (Some(1), Some(1), Some(1), Some(2))
    );
}

#[tokio::test]
async fn uninstrumented_and_over_budget_windows_do_not_report_fake_zero_or_full_counts() {
    let (pool, repo) = setup().await;
    source(
        &pool,
        "standard",
        "unit_standard",
        json!({"symbol":"kg","base_unit":"g","factor":1000.0}),
    )
    .await;
    repo.save(&entry("catalog_standard", "unit_standard", "standard"))
        .await
        .expect("standard asset");
    let uninstrumented = repo
        .analyze_usage(&usage("catalog_standard"))
        .await
        .expect("explicit unknown source");
    assert!(!uninstrumented.complete);
    assert_eq!(uninstrumented.instrumentation, "not_instrumented");
    assert_eq!(uninstrumented.matched_recorded_events, None);
    assert_eq!(uninstrumented.succeeded, None);
    publish(&repo, "catalog").await;
    let params = usage("catalog");
    sqlx::query(
        "WITH RECURSIVE seq(n) AS(SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<5001)
        INSERT INTO data_access_audit(id,tenant_id,actor_id,action,outcome,evidence,created_at)
        SELECT 'large_'||n,'a','a-requester','query','succeeded',?1,?2 FROM seq",
    )
    .bind(json!({"metadata_ids":["metadata-a"]}).to_string())
    .bind((params.request.period_start + Duration::minutes(1)).to_rfc3339())
    .execute(&pool)
    .await
    .expect("large actual audit fixture");
    let capped = repo.analyze_usage(&params).await.expect("bounded usage");
    assert!(!capped.complete);
    assert_eq!(capped.matched_recorded_events, Some(5001));
    assert_eq!(capped.counted_recorded_events, 0);
    assert_eq!(capped.succeeded, None);
}

#[tokio::test]
async fn published_claim_without_actual_model_receipt_is_never_verified() {
    let (pool, repo) = setup().await;
    source(
        &pool,
        "model",
        "data_model",
        json!({"table_name":"main.orders","columns":[{"name":"id","data_type":"BIGINT"}]}),
    )
    .await;
    sqlx::query("UPDATE product_resource SET status='published' WHERE id='model'")
        .execute(&pool)
        .await
        .expect("legacy claimed published");
    let draft = repo
        .save(&entry("model_asset", "data_model", "model"))
        .await
        .expect("real object registration draft");
    assert!(
        transition(&repo, &draft, AssetTransition::Register)
            .await
            .is_err()
    );
    let view = repo
        .get_asset(&read("model_asset"))
        .await
        .expect("view")
        .expect("exists");
    assert!(!view.source_valid);
    assert_eq!(
        view.invalid_reason.as_deref(),
        Some("source_missing_or_unverified")
    );
    let now = Utc::now();
    let invalid = AssetUsageRequest {
        asset_id: "model_asset".into(),
        period_start: now,
        period_end: now,
    };
    assert!(validation::usage(&invalid).is_err());
}
