// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod product_job_support;
use dts_backend::{
    BackendError,
    repo::{metadata_catalog::*, product_job::*, product_resource::*},
};
use dts_backend_sqlite::{
    SqliteMetadataCatalogRepo, SqliteProductJobRepo, SqliteProductResourceRepo,
};
use serde_json::json;
use sqlx::SqlitePool;

struct Fixture {
    pool: SqlitePool,
    jobs: SqliteProductJobRepo,
    catalog: SqliteMetadataCatalogRepo,
    resources: SqliteProductResourceRepo,
}
async fn setup() -> Fixture {
    let (pool, jobs) = product_job_support::setup().await;
    sqlx::query("INSERT INTO role(id,tenant_id,name) VALUES('metadata-role','a','collector')")
        .execute(&pool)
        .await
        .unwrap();
    for permission in ["metadata:read", "metadata:execute"] {
        sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES('metadata-role',?1)")
            .bind(permission)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES('a-owner','metadata-role')")
        .execute(&pool)
        .await
        .unwrap();
    Fixture {
        catalog: SqliteMetadataCatalogRepo::new(pool.clone()),
        resources: SqliteProductResourceRepo::new(pool.clone()),
        pool,
        jobs,
    }
}
fn table(name: &str) -> MetadataTable {
    MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: name.into(),
        table_type: "BASE TABLE".into(),
        columns: vec![MetadataColumn {
            name: "id".into(),
            data_type: "BIGINT".into(),
            nullable: false,
            ordinal_position: 1,
        }],
    }
}
async fn running(fixture: &Fixture, key: &str, tables: Vec<MetadataTable>) -> MetadataApplyParams {
    let scope = MetadataScope {
        engine: "swanlake".into(),
        table_catalog: None,
        schema_name: None,
        table_name: None,
    };
    let source_id = format!("collection-{key}");
    let properties = json!({"source_type":"swanlake"});
    fixture
        .resources
        .create(&CreateResourceParams {
            id: source_id.clone(),
            tenant_id: "a".into(),
            kind: "metadata_collection".into(),
            actor_id: "a-owner".into(),
            fields: ResourceWrite {
                code: source_id.clone(),
                name: "Collection fixture".into(),
                description: None,
                parent_id: None,
                status: "draft".into(),
                properties: properties.clone(),
                references: vec![],
            },
        })
        .await
        .unwrap();
    let job=fixture.jobs.enqueue(&EnqueueProductJobParams{id:key.into(),tenant_id:"a".into(),source_kind:"metadata_collection".into(),source_id:source_id.clone(),source_version:1,
        actor_id:"a-owner".into(),action:"collect_metadata".into(),idempotency_key:key.into(),definition_snapshot:json!({"format_version":1,"executor":"metadata_catalog_v1",
            "tenant_id":"a","source_kind":"metadata_collection","source_id":source_id,"source_version":1,"properties":properties,"scope":scope}),
        parameters:json!({}),can_retry:true,nodes:vec![ProductJobNodeDefinition{id:"collect".into(),name:"Collect".into(),definition:json!({"type":"metadata_information_schema","scope":scope}),depends_on:vec![]}]}).await.unwrap().job;
    let claimed = fixture
        .jobs
        .claim(&ClaimProductJobParams {
            tenant_id: "a".into(),
            worker_id: "collector".into(),
            lease_seconds: 30,
            source_kinds: vec!["metadata_collection".into()],
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job.id);
    let lease = ProductJobLease {
        worker_id: claimed.lease_owner.unwrap(),
        fence: claimed.lease_fence,
    };
    fixture
        .jobs
        .start_node(&StartProductJobNodeParams {
            tenant_id: "a".into(),
            job_id: job.id.clone(),
            node_id: "collect".into(),
            lease: lease.clone(),
        })
        .await
        .unwrap();
    MetadataApplyParams {
        tenant_id: "a".into(),
        actor_id: "a-owner".into(),
        collection_id: source_id,
        collection_version: 1,
        collection_properties: properties,
        scope,
        tables,
        job_id: job.id,
        node_id: "collect".into(),
        lease,
    }
}
async fn object_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM product_resource WHERE kind='metadata_object'")
        .fetch_one(&fixture.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_late_storage_failure_rolls_back_all_objects_audits_and_node_success() {
    let fixture = setup().await;
    let params = running(&fixture, "rollback", vec![table("first"), table("broken")]).await;
    sqlx::raw_sql("CREATE TRIGGER reject_metadata_test BEFORE INSERT ON metadata_catalog WHEN NEW.table_name='broken'
        BEGIN SELECT RAISE(ABORT,'controlled metadata failure'); END;").execute(&fixture.pool).await.unwrap();
    assert!(fixture.catalog.apply_snapshot(&params).await.is_err());
    assert_eq!(object_count(&fixture).await, 0);
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_resource_audit WHERE resource_kind='metadata_object'",
    )
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(audits, 0);
    let before = fixture
        .jobs
        .get(&product_job_support::lookup("rollback"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.nodes[0].status, ProductJobNodeStatus::Running);
    assert!(before.nodes[0].result.is_none());
    sqlx::raw_sql("DROP TRIGGER reject_metadata_test")
        .execute(&fixture.pool)
        .await
        .unwrap();
    let result = fixture.catalog.apply_snapshot(&params).await.unwrap();
    assert_eq!(result.created_objects, 2);
    let committed = fixture
        .jobs
        .get(&product_job_support::lookup("rollback"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(committed.nodes[0].status, ProductJobNodeStatus::Succeeded);
    assert_eq!(
        committed.nodes[0].result.as_ref().unwrap()["reflection_committed"],
        true
    );
}

#[tokio::test]
async fn old_fences_expired_leases_and_cancelled_jobs_cannot_commit_reflection() {
    let fixture = setup().await;
    let mut params = running(&fixture, "fence", vec![table("orders")]).await;
    product_job_support::expire(&fixture.pool, &params.job_id).await;
    assert!(matches!(
        fixture.catalog.apply_snapshot(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(object_count(&fixture).await, 0);
    let claimed = fixture
        .jobs
        .claim(&ClaimProductJobParams {
            tenant_id: "a".into(),
            worker_id: "next-collector".into(),
            lease_seconds: 30,
            source_kinds: vec!["metadata_collection".into()],
        })
        .await
        .unwrap()
        .unwrap();
    let new_lease = ProductJobLease {
        worker_id: claimed.lease_owner.unwrap(),
        fence: claimed.lease_fence,
    };
    fixture
        .jobs
        .start_node(&StartProductJobNodeParams {
            tenant_id: "a".into(),
            job_id: params.job_id.clone(),
            node_id: "collect".into(),
            lease: new_lease.clone(),
        })
        .await
        .unwrap();
    assert!(fixture.catalog.apply_snapshot(&params).await.is_err());
    params.lease = new_lease;
    fixture
        .jobs
        .request_cancel(&CancelProductJobParams {
            tenant_id: "a".into(),
            job_id: params.job_id.clone(),
            actor_id: "a-owner".into(),
        })
        .await
        .unwrap();
    assert!(matches!(
        fixture.catalog.apply_snapshot(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(object_count(&fixture).await, 0);
}

#[tokio::test]
async fn inactive_actor_changed_source_and_forged_scope_are_rejected_before_any_reflection() {
    let fixture = setup().await;
    let mut params = running(&fixture, "guards", vec![table("orders")]).await;
    params.scope.schema_name = Some("other".into());
    assert!(fixture.catalog.apply_snapshot(&params).await.is_err());
    params.scope.schema_name = None;
    sqlx::query("UPDATE \"user\" SET status='inactive' WHERE id='a-owner'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(fixture.catalog.apply_snapshot(&params).await.is_err());
    sqlx::query("UPDATE \"user\" SET status='active' WHERE id='a-owner'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_permission WHERE role_id='metadata-role' AND permission='metadata:execute'").execute(&fixture.pool).await.unwrap();
    assert!(fixture.catalog.apply_snapshot(&params).await.is_err());
    sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES('metadata-role','metadata:execute')").execute(&fixture.pool).await.unwrap();
    sqlx::query("UPDATE product_resource SET version=2 WHERE kind='metadata_collection' AND id=?1")
        .bind(&params.collection_id)
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(matches!(
        fixture.catalog.apply_snapshot(&params).await,
        Err(BackendError::Conflict(_))
    ));
    assert_eq!(object_count(&fixture).await, 0);
}

#[tokio::test]
async fn a_user_supplied_binding_has_no_registry_proof_and_changed_columns_invalidate_existing_proof()
 {
    let fixture = setup().await;
    let params = running(&fixture, "binding", vec![table("orders")]).await;
    fixture.catalog.apply_snapshot(&params).await.unwrap();
    let id: String =
        sqlx::query_scalar("SELECT object_id FROM metadata_catalog WHERE tenant_id='a'")
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert!(
        fixture
            .catalog
            .verified_binding("a", &id, 1)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        fixture
            .catalog
            .verified_binding("b", &id, 1)
            .await
            .unwrap()
            .is_none()
    );
    let mut properties = table("orders").properties("a").unwrap();
    properties["columns"][0]["nullable"] = json!(true);
    sqlx::query("UPDATE product_resource SET properties=?1 WHERE id=?2")
        .bind(properties.to_string())
        .bind(&id)
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(
        fixture
            .catalog
            .verified_binding("a", &id, 1)
            .await
            .unwrap()
            .is_none()
    );
    let fake = table("manual").properties("a").unwrap();
    fixture
        .resources
        .create(&CreateResourceParams {
            id: "manual-metadata".into(),
            tenant_id: "a".into(),
            kind: "metadata_object".into(),
            actor_id: "a-owner".into(),
            fields: ResourceWrite {
                code: "manual".into(),
                name: "Manual".into(),
                description: None,
                parent_id: None,
                status: "published".into(),
                properties: fake,
                references: vec![],
            },
        })
        .await
        .unwrap();
    assert!(
        fixture
            .catalog
            .verified_binding("a", "manual-metadata", 1)
            .await
            .unwrap()
            .is_none()
    );
}
