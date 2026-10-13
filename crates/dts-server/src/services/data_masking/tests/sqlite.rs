use super::*;
use dts_backend::repo::metadata_catalog::MetadataTable;
use dts_backend::{CreateResourceParams, ProductResourceRepo, ResourceReference, ResourceWrite};
use dts_backend_sqlite::{SqliteMetadataCatalogRepo, SqliteProductResourceRepo, SqliteUserRepo};

async fn setup() -> (sqlx::SqlitePool, super::super::MaskingService, AuthIdentity) {
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
        include_str!("../../../../../dts-backend-sqlite/migrations/011_metadata_catalog.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','T','t'),('other','O','o');
        INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('user','tenant','manager','m@test','hash');
        INSERT INTO role(id,tenant_id,name) VALUES('privacy-role','tenant','privacy-manager');
        INSERT INTO user_role(user_id,role_id) VALUES('user','privacy-role');
        INSERT INTO role_permission(role_id,permission) VALUES('privacy-role','privacy:write');")
        .execute(&pool).await.unwrap();
    let resources = Arc::new(SqliteProductResourceRepo::new(pool.clone()));
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "people".into(),
        table_type: "BASE TABLE".into(),
        columns: target().columns,
    };
    for (id, kind, properties, references) in [
        (
            "metadata-1",
            "metadata_object",
            table.properties("tenant").unwrap(),
            vec![],
        ),
        (
            "rule-1",
            "masking_rule",
            json!({"object_id":"metadata-1","column_name":"email","method":"hash","parameters":{},
            "metadata_version":999,"column_data_type":"FAKE","source_fingerprint":"f".repeat(64)}),
            vec![ResourceReference {
                field: "object_id".into(),
                target_kind: "metadata_object".into(),
                target_id: "metadata-1".into(),
            }],
        ),
        (
            "group-1",
            "user_group",
            json!({"member_ids":["user"],"masking_rule_id":"rule-1"}),
            vec![
                ResourceReference {
                    field: "member_ids".into(),
                    target_kind: "user".into(),
                    target_id: "user".into(),
                },
                ResourceReference {
                    field: "masking_rule_id".into(),
                    target_kind: "masking_rule".into(),
                    target_id: "rule-1".into(),
                },
            ],
        ),
    ] {
        resources
            .create(&CreateResourceParams {
                id: id.into(),
                tenant_id: "tenant".into(),
                kind: kind.into(),
                actor_id: "user".into(),
                fields: ResourceWrite {
                    code: id.replace('-', "_"),
                    name: id.into(),
                    description: None,
                    parent_id: None,
                    status: if kind == "metadata_object" {
                        "published"
                    } else {
                        "draft"
                    }
                    .into(),
                    properties,
                    references,
                },
            })
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
        VALUES('tenant','swanlake','lake','main','people','metadata-1',1,'BASE TABLE',?1,?2,'present','test-collector',CURRENT_TIMESTAMP)")
        .bind(serde_json::to_string(&table.columns).unwrap()).bind(table.fingerprint().unwrap()).execute(&pool).await.unwrap();
    let service = super::super::MaskingService::new(
        resources,
        Arc::new(SqliteMetadataCatalogRepo::new(pool.clone())),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        PrivacyKey::from_bytes([11; 32]),
    );
    (
        pool,
        service,
        AuthIdentity {
            sub: "user".into(),
            tenant_id: "tenant".into(),
            username: "manager".into(),
        },
    )
}

#[tokio::test]
async fn only_real_published_groups_apply_and_signed_history_does_not_hash_twice() {
    let (pool, service, actor) = setup().await;
    let query = AuthorizedQuery::masking_test_token(
        &actor,
        "SELECT email AS contact FROM lake.main.people",
    );
    assert!(
        !service
            .prepare_output(&actor, &query)
            .await
            .unwrap()
            .has_masks()
    );
    assert!(
        service
            .publish(&actor, "user_group", "group-1", 1)
            .await
            .is_err()
    );
    let published = service
        .publish(&actor, "masking_rule", "rule-1", 1)
        .await
        .unwrap();
    assert_eq!(published.properties["metadata_version"], 1);
    assert_eq!(published.properties["column_data_type"], "VARCHAR");
    assert_ne!(published.properties["source_fingerprint"], "f".repeat(64));
    assert!(
        !service
            .prepare_output(&actor, &query)
            .await
            .unwrap()
            .has_masks()
    );
    service
        .publish(&actor, "user_group", "group-1", 1)
        .await
        .unwrap();
    let plan = service.prepare_output(&actor, &query).await.unwrap();
    assert!(plan.has_masks());
    let masked = service
        .apply_output(&plan, result(json!("private-value")))
        .unwrap();
    let masked_value = masked.rows[0]["contact"].clone();
    let receipt = service.seal_output(&plan, &masked).unwrap();
    let same = service
        .apply_stored_output(&actor, &query, masked, Some(&receipt))
        .await
        .unwrap();
    assert_eq!(same.rows[0]["contact"], masked_value);
    assert!(
        service
            .apply_stored_output(
                &actor,
                &query,
                result(json!("forged raw value")),
                Some(&receipt)
            )
            .await
            .is_err()
    );
    let audits: Vec<String> =
        sqlx::query_scalar("SELECT summary FROM product_resource_audit WHERE tenant_id='tenant'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!audits.join("").contains("private-value"));
    service
        .withdraw(&actor, "masking_rule", "rule-1", 2)
        .await
        .unwrap();
    assert!(service.reauthorize_plan(&actor, &plan).await.is_err());
    assert!(
        service
            .apply_stored_output(&actor, &query, same, Some(&receipt))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn real_registry_scope_user_status_and_manager_permission_are_enforced() {
    let (pool, service, actor) = setup().await;
    let other = AuthIdentity {
        tenant_id: "other".into(),
        ..actor.clone()
    };
    assert!(
        service
            .validate_saved(&other, "masking_rule", "rule-1")
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM role_permission WHERE role_id='privacy-role'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .publish(&actor, "masking_rule", "rule-1", 1)
            .await
            .is_err()
    );
    sqlx::query(
        "INSERT INTO role_permission(role_id,permission) VALUES('privacy-role','privacy:write')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE metadata_catalog SET discovery_status='missing' WHERE tenant_id='tenant'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        service
            .publish(&actor, "masking_rule", "rule-1", 1)
            .await
            .is_err()
    );
    sqlx::query("UPDATE metadata_catalog SET discovery_status='present' WHERE tenant_id='tenant'")
        .execute(&pool)
        .await
        .unwrap();
    service
        .publish(&actor, "masking_rule", "rule-1", 1)
        .await
        .unwrap();
    service
        .publish(&actor, "user_group", "group-1", 1)
        .await
        .unwrap();
    let query = AuthorizedQuery::masking_test_token(
        &actor,
        "SELECT email AS contact FROM lake.main.people",
    );
    let plan = service.prepare_output(&actor, &query).await.unwrap();
    sqlx::query("UPDATE \"user\" SET status='disabled' WHERE id='user'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(service.reauthorize_plan(&actor, &plan).await.is_err());
}
