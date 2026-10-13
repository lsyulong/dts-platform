use super::*;
use sqlx::Row;

#[tokio::test]
async fn compiled_quality_queries_count_real_null_duplicate_range_and_expression_rows() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE fixture(id BIGINT,amount DOUBLE,status TEXT); INSERT INTO fixture VALUES(1,10,'active'),(1,20,'inactive'),(2,NULL,'active'),(NULL,5,'active');").execute(&pool).await.unwrap();
    let cases = [
        ("not_null", "amount", json!([]), 1i64),
        ("unique", "id", json!([]), 3),
        (
            "range",
            "amount",
            json!([{"column":"amount","operator":"gte","value":10},{"column":"amount","operator":"lte","value":20}]),
            2,
        ),
        (
            "expression",
            "status",
            json!([{"column":"status","operator":"eq","value":"active"}]),
            1,
        ),
    ];
    for (kind, column, conditions, failed) in cases {
        let definition = definition(kind, column, conditions);
        let sql = compile_node(&definition).unwrap();
        let row = sqlx::query(&sql).fetch_one(&pool).await.unwrap();
        assert_eq!(row.get::<i64, _>("checked_rows"), 4);
        assert_eq!(row.get::<i64, _>("failed_rows"), failed);
        let result = map_node_result(&definition, counts(json!(4), json!(failed))).unwrap();
        assert_eq!(result["status"], "failed");
    }
}

#[tokio::test]
async fn hostile_string_literal_is_data_and_cannot_terminate_the_count_query() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE fixture(id BIGINT,amount DOUBLE,status TEXT);")
        .execute(&pool)
        .await
        .unwrap();
    for literal in [
        "x'; DROP TABLE fixture; --",
        "back\\slash ' quote",
        "双引号\"与单引号'",
    ] {
        sqlx::query("INSERT INTO fixture VALUES(1,1,?1)")
            .bind(literal)
            .execute(&pool)
            .await
            .unwrap();
        let definition = definition(
            "expression",
            "status",
            json!([{"column":"status","operator":"eq","value":literal}]),
        );
        let row = sqlx::query(&compile_node(&definition).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fixture")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.get::<i64, _>("checked_rows"), count);
        assert_eq!(row.get::<i64, _>("failed_rows"), count - 1);
    }
}

#[tokio::test]
async fn quality_enqueue_persists_frozen_refs_and_rejects_cross_tenant_or_engine_mismatch() {
    use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataTable};
    use dts_backend::{CreateResourceParams, ResourceWrite};
    use dts_backend_sqlite::{
        SqliteMetadataCatalogRepo, SqliteProductJobRepo, SqliteProductResourceRepo,
    };
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .unwrap();
    for sql in [
        include_str!("../../../../../dts-backend-sqlite/migrations/001_initial_schema.sql"),
        include_str!("../../../../../dts-backend-sqlite/migrations/002_product_resources.sql"),
        include_str!("../../../../../dts-backend-sqlite/migrations/004_product_external_refs.sql"),
        include_str!("../../../../../dts-backend-sqlite/migrations/006_product_jobs.sql"),
        include_str!("../../../../../dts-backend-sqlite/migrations/011_metadata_catalog.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO tenant(id,name,slug) VALUES('tenant-a','A','a'),('tenant-b','B','b')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor-1','tenant-a','actor','actor@test','hash')").execute(&pool).await.unwrap();
    let resources = Arc::new(SqliteProductResourceRepo::new(pool.clone()));
    let mut frozen = snapshot();
    let table = MetadataTable {
        table_catalog: "warehouse".into(),
        schema_name: "main".into(),
        table_name: "fixture".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![
            MetadataColumn {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: true,
                ordinal_position: 1,
            },
            MetadataColumn {
                name: "amount".into(),
                data_type: "DOUBLE".into(),
                nullable: true,
                ordinal_position: 2,
            },
            MetadataColumn {
                name: "status".into(),
                data_type: "VARCHAR".into(),
                nullable: false,
                ordinal_position: 3,
            },
        ],
    };
    frozen.metadata.properties = table.properties("tenant-a").unwrap();
    for (kind, resource) in [
        ("metadata_object", &frozen.metadata),
        ("quality_object", &frozen.object),
        ("quality_rule", &frozen.rules[0]),
        ("quality_task", &frozen.task),
    ] {
        resources
            .create(&CreateResourceParams {
                id: resource.id.clone(),
                tenant_id: "tenant-a".into(),
                kind: kind.into(),
                actor_id: "actor-1".into(),
                fields: ResourceWrite {
                    code: resource.id.replace('-', "_"),
                    name: resource.name.clone(),
                    description: None,
                    parent_id: None,
                    status: if kind == "metadata_object" {
                        "published"
                    } else {
                        "draft"
                    }
                    .into(),
                    properties: resource.properties.clone(),
                    references: vec![],
                },
            })
            .await
            .unwrap();
    }
    let jobs = Arc::new(SqliteProductJobRepo::new(pool.clone()));
    let actor = AuthIdentity {
        sub: "actor-1".into(),
        tenant_id: "tenant-a".into(),
        username: "actor".into(),
    };
    let service = QualityService::new(
        resources.clone(),
        jobs.clone(),
        QueryAccess {
            tenant_id: Some("tenant-a".into()),
        },
    );
    assert!(
        !service
            .validate_saved(&actor, "quality_task", "task-1", 1)
            .await
            .unwrap()
            .engine_bound
    );
    let service =
        service.with_metadata_repo(Arc::new(SqliteMetadataCatalogRepo::new(pool.clone())));
    assert!(
        service
            .enqueue(
                &actor,
                "task-1",
                QualityRunRequest {
                    expected_version: 1,
                    idempotency_key: "unverified".into()
                }
            )
            .await
            .is_err()
    );
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
        VALUES('tenant-a','swanlake','warehouse','main','fixture','metadata-1',1,'BASE TABLE',?1,?2,'present','fixture-collector',CURRENT_TIMESTAMP)")
        .bind(serde_json::to_string(&table.columns).unwrap()).bind(table.fingerprint().unwrap()).execute(&pool).await.unwrap();
    assert!(
        service
            .validate_saved(&actor, "quality_task", "task-1", 1)
            .await
            .unwrap()
            .engine_bound
    );
    let enqueued = service
        .enqueue(
            &actor,
            "task-1",
            QualityRunRequest {
                expected_version: 1,
                idempotency_key: "run-1".into(),
            },
        )
        .await
        .unwrap();
    assert!(enqueued.created);
    assert!(enqueued.job.can_retry);
    assert_eq!(prepare_claimed(&enqueued.job).unwrap().len(), 1);
    assert_eq!(enqueued.job.definition_snapshot["rules"][0]["version"], 1);
    let wrong_engine = QualityService::new(
        resources.clone(),
        jobs.clone(),
        QueryAccess {
            tenant_id: Some("tenant-b".into()),
        },
    );
    assert!(
        wrong_engine
            .enqueue(
                &actor,
                "task-1",
                QualityRunRequest {
                    expected_version: 1,
                    idempotency_key: "run-2".into()
                }
            )
            .await
            .is_err()
    );
    let foreign = AuthIdentity {
        tenant_id: "tenant-b".into(),
        ..actor
    };
    assert!(
        service
            .validate_saved(&foreign, "quality_task", "task-1", 1)
            .await
            .is_err()
    );
}
