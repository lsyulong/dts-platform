use chrono::Utc;
use dts_backend::{
    CreateResourceParams, ProductResourceRepo, ResourceReference, ResourceWrite,
    UpdateResourceParams,
    repo::{metadata_catalog::*, metadata_lineage::*},
};
use dts_backend_sqlite::SqliteProductResourceRepo;
use serde_json::json;
use sqlx::SqlitePool;

mod data_access_support;

async fn setup() -> (SqlitePool, SqliteProductResourceRepo) {
    let (pool, _, target) = data_access_support::setup().await;
    data_access_support::permission(
        &pool,
        "a-requester",
        "metadata_reader",
        "metadata:read",
        false,
    )
    .await;
    sqlx::query(
        "INSERT INTO role(id,tenant_id,name) VALUES('b-metadata-reader','b','metadata_reader')",
    )
    .execute(&pool)
    .await
    .expect("b role");
    sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES('b-metadata-reader','metadata:read')").execute(&pool).await.expect("b permission");
    sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES('b-requester','b-metadata-reader')")
        .execute(&pool)
        .await
        .expect("b membership");
    for id in ["metadata-b", "metadata-c"] {
        let table = MetadataTable {
            table_catalog: "warehouse".into(),
            schema_name: "main".into(),
            table_name: id.into(),
            table_type: "BASE TABLE".into(),
            columns: target.columns.clone(),
        };
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at)
            VALUES(?1,'a','metadata_object',?2,?1,'published',?3,1,'a-approver',?4,?4)")
            .bind(id).bind(id.replace('-',"_")).bind(table.properties("a").expect("properties").to_string()).bind(Utc::now().to_rfc3339()).execute(&pool).await.expect("metadata resource");
        sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
            VALUES('a','swanlake','warehouse','main',?1,?1,1,'BASE TABLE',?2,?3,'present','collection',?4)")
            .bind(id).bind(json!(table.columns).to_string()).bind(table.fingerprint().expect("fingerprint")).bind(Utc::now().to_rfc3339()).execute(&pool).await.expect("collector proof");
    }
    let repo = SqliteProductResourceRepo::new(pool.clone());
    (pool, repo)
}
fn edge(id: &str, source: &str, target: &str) -> CreateResourceParams {
    CreateResourceParams {
        id: id.into(),
        tenant_id: "a".into(),
        kind: "metadata_lineage".into(),
        actor_id: "a-requester".into(),
        fields: ResourceWrite {
            code: id.replace('-', "_"),
            name: id.into(),
            description: None,
            parent_id: None,
            status: "draft".into(),
            properties: json!({"source_object_id":source,"target_object_id":target,"source_object_version":1,"target_object_version":1,"origin":"manual","evidence":"Owner verified actual relation",
            "evidence_actor_id":"a-requester","evidence_recorded_at":Utc::now()}),
            references: vec![
                ResourceReference {
                    field: "source_object_id".into(),
                    target_kind: "metadata_object".into(),
                    target_id: source.into(),
                },
                ResourceReference {
                    field: "target_object_id".into(),
                    target_kind: "metadata_object".into(),
                    target_id: target.into(),
                },
                ResourceReference {
                    field: "evidence_actor_id".into(),
                    target_kind: "user".into(),
                    target_id: "a-requester".into(),
                },
            ],
        },
    }
}
fn snapshot() -> LineageSnapshotParams {
    LineageSnapshotParams {
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        max_edges: 2000,
        root_object_id: Some("metadata-a".into()),
    }
}

#[tokio::test]
async fn endpoints_evidence_and_actor_are_verified_in_the_writing_transaction() {
    let (pool, repo) = setup().await;
    let mut fake = edge("fake", "metadata-a", "metadata-b");
    fake.fields.properties["origin"] = json!("sql");
    assert!(repo.create(&fake).await.is_err());
    fake = edge("fake", "metadata-a", "metadata-b");
    fake.fields.properties["evidence_actor_id"] = json!("a-other");
    assert!(repo.create(&fake).await.is_err());
    fake = edge("fake", "metadata-a", "metadata-b");
    fake.fields.properties["source_object_version"] = json!(2);
    assert!(repo.create(&fake).await.is_err());
    fake = edge("fake", "metadata-a", "unknown");
    assert!(repo.create(&fake).await.is_err());
    fake = edge("fake", "metadata-a", "metadata-b");
    fake.tenant_id = "b".into();
    fake.actor_id = "b-requester".into();
    fake.fields.properties["evidence_actor_id"] = json!("b-requester");
    fake.fields.references[2].target_id = "b-requester".into();
    assert!(repo.create(&fake).await.is_err());
    assert!(
        repo.create(&edge("self", "metadata-a", "metadata-a"))
            .await
            .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE kind='metadata_lineage'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 0);
}
#[tokio::test]
async fn duplicate_edges_and_cycles_are_rejected_atomically() {
    let (pool, repo) = setup().await;
    let first = repo
        .create(&edge("ab", "metadata-a", "metadata-b"))
        .await
        .expect("a-b");
    repo.create(&edge("bc", "metadata-b", "metadata-c"))
        .await
        .expect("b-c");
    assert!(
        repo.create(&edge("ca", "metadata-c", "metadata-a"))
            .await
            .is_err()
    );
    assert!(
        repo.create(&edge("duplicate", "metadata-a", "metadata-b"))
            .await
            .is_err()
    );
    let reverse = edge("unused", "metadata-c", "metadata-a");
    assert!(
        repo.update(
            "a",
            "metadata_lineage",
            "ab",
            &UpdateResourceParams {
                expected_version: first.version,
                actor_id: "a-requester".into(),
                fields: reverse.fields
            }
        )
        .await
        .is_ok()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE kind='metadata_lineage'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count, 2);
    assert!(
        sqlx::query("DELETE FROM product_resource WHERE id='metadata-a'")
            .execute(&pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn opposite_concurrent_edges_cannot_both_commit() {
    let (_, repo) = setup().await;
    let a = edge("ab", "metadata-a", "metadata-b");
    let b = edge("ba", "metadata-b", "metadata-a");
    let (forward, backward) = tokio::join!(repo.create(&a), repo.create(&b));
    assert_ne!(forward.is_ok(), backward.is_ok());
    let graph = repo.snapshot(&snapshot()).await.expect("graph");
    assert_eq!(graph.total_edges, 1);
    assert_eq!(graph.edges.len(), 1);
}
#[tokio::test]
async fn graph_reads_true_nodes_and_marks_version_changed_evidence_stale() {
    let (pool, repo) = setup().await;
    repo.create(&edge("ab", "metadata-a", "metadata-b"))
        .await
        .expect("edge");
    let graph = repo.snapshot(&snapshot()).await.expect("graph");
    assert_eq!(graph.nodes.len(), 2);
    assert!(!graph.edges[0].stale);
    assert_eq!(graph.edges[0].evidence_actor_id, "a-requester");
    sqlx::query("UPDATE product_resource SET version=2 WHERE id='metadata-b'")
        .execute(&pool)
        .await
        .expect("new source version");
    sqlx::query("UPDATE metadata_catalog SET object_version=2 WHERE object_id='metadata-b'")
        .execute(&pool)
        .await
        .expect("new reflected version");
    let stale = repo.snapshot(&snapshot()).await.expect("stale");
    assert!(stale.edges[0].stale);
    assert_eq!(
        stale.edges[0].invalid_reason.as_deref(),
        Some("endpoint_version_changed")
    );
    assert_eq!(
        stale
            .nodes
            .iter()
            .find(|node| node.id == "metadata-b")
            .expect("current real object")
            .version,
        2
    );
    let other = repo
        .snapshot(&LineageSnapshotParams {
            tenant_id: "b".into(),
            actor_id: "b-requester".into(),
            max_edges: 100,
            root_object_id: Some("metadata-a".into()),
        })
        .await
        .expect("other tenant");
    assert!(other.nodes.is_empty());
    assert!(other.edges.is_empty());
}
#[tokio::test]
async fn isolated_roots_scan_truncation_and_legacy_provenance_are_truthful() {
    let (pool, repo) = setup().await;
    let isolated = repo.snapshot(&snapshot()).await.expect("isolated");
    assert_eq!(isolated.nodes.len(), 1);
    assert_eq!(isolated.total_edges, 0);
    repo.create(&edge("ab", "metadata-a", "metadata-b"))
        .await
        .expect("ab");
    repo.create(&edge("bc", "metadata-b", "metadata-c"))
        .await
        .expect("bc");
    let limited = repo
        .snapshot(&LineageSnapshotParams {
            max_edges: 1,
            ..snapshot()
        })
        .await
        .expect("limited");
    assert!(limited.scan_truncated);
    assert_eq!(limited.scanned_edges, 1);
    assert_eq!(limited.total_edges, 2);
    sqlx::query("UPDATE product_resource SET properties=json_remove(properties,'$.evidence_actor_id') WHERE id='ab'").execute(&pool).await.expect("old unverified fixture");
    let legacy = repo.snapshot(&snapshot()).await.expect("legacy excluded");
    assert_eq!(legacy.invalid_edges.len(), 1);
    assert_eq!(legacy.invalid_edges[0].id, "ab");
    assert_eq!(legacy.edges.len(), 1);
    assert!(!legacy.scan_truncated);
    sqlx::query("DELETE FROM role_permission WHERE role_id='a-requester-metadata_reader'")
        .execute(&pool)
        .await
        .expect("permission revoked");
    assert!(repo.snapshot(&snapshot()).await.is_err());
}
