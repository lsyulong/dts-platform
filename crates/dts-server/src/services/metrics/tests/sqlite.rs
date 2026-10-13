use super::*;
use crate::{
    models::auth::AuthIdentity,
    services::{auth_svc::AuthService, sa_auth},
};
use dts_backend::{
    CreateResourceParams, ProductResource, ResourceWrite, UpdateResourceParams,
    repo::metadata_catalog::{MetadataColumn, MetadataTable},
};
use dts_backend_sqlite::{
    SqliteDataAccessRepo, SqliteMetadataCatalogRepo, SqliteProductJobRepo,
    SqliteProductResourceRepo, SqliteRoleRepo, SqliteTenantRepo, SqliteUserRepo,
};
use serde_json::{Value, json};

struct Fixture {
    pool: sqlx::SqlitePool,
    resources: Arc<SqliteProductResourceRepo>,
    service: MetricsService,
    actor: AuthIdentity,
}
async fn create(
    resources: &SqliteProductResourceRepo,
    kind: &str,
    id: &str,
    status: &str,
    properties: Value,
) -> ProductResource {
    resources
        .create(&CreateResourceParams {
            id: id.into(),
            tenant_id: "tenant".into(),
            kind: kind.into(),
            actor_id: "actor".into(),
            fields: ResourceWrite {
                code: id.replace('-', "_"),
                name: id.into(),
                description: None,
                parent_id: None,
                status: status.into(),
                properties,
                references: vec![],
            },
        })
        .await
        .unwrap()
}
async fn setup() -> Fixture {
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
        include_str!("../../../../../dts-backend-sqlite/migrations/012_data_access.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'),('other','Other','other'); INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor','tenant','actor','a@test','hash'); INSERT INTO role(id,tenant_id,name,is_system) VALUES('admin','tenant','tenant_admin',1); INSERT INTO user_role(user_id,role_id) VALUES('actor','admin'); INSERT INTO role_permission(role_id,permission) VALUES('admin','metrics:read'),('admin','metrics:write'),('admin','metrics:execute'),('admin','data_security:all_data');").execute(&pool).await.unwrap();
    let resources = Arc::new(SqliteProductResourceRepo::new(pool.clone()));
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: [
            ("id", "BIGINT", false),
            ("event_time", "TIMESTAMPTZ", false),
            ("region", "VARCHAR", true),
            ("amount", "DECIMAL(18,2)", true),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, ty, nullable))| MetadataColumn {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            ordinal_position: index as u32 + 1,
        })
        .collect(),
    };
    create(
        &resources,
        "metadata_object",
        "metadata",
        "published",
        table.properties("tenant").unwrap(),
    )
    .await;
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at) VALUES('tenant','swanlake','lake','main','orders','metadata',1,'BASE TABLE',?1,?2,'present','fixture-collection',CURRENT_TIMESTAMP)")
        .bind(serde_json::to_string(&table.columns).unwrap()).bind(table.fingerprint().unwrap()).execute(&pool).await.unwrap();
    create(
        &resources,
        "warehouse_process",
        "process",
        "active",
        json!({"domain_id":"domain","grain":"订单主键"}),
    )
    .await;
    let mut model=create(&resources,"data_model","model","draft",json!({"layer_id":"layer","table_name":"orders","primary_key":["id"],"columns":table.columns.iter().map(|column|json!({"name":column.name,"data_type":column.data_type,"nullable":column.nullable})).collect::<Vec<_>>()})).await;
    for status in ["publishing", "published"] {
        model = resources
            .update(
                "tenant",
                "data_model",
                "model",
                &UpdateResourceParams {
                    expected_version: model.version,
                    actor_id: "actor".into(),
                    fields: ResourceWrite {
                        code: model.code,
                        name: model.name,
                        description: None,
                        parent_id: None,
                        status: status.into(),
                        properties: model.properties,
                        references: vec![],
                    },
                },
            )
            .await
            .unwrap();
    }
    resources.create(&CreateResourceParams{id:"model-receipt".into(),tenant_id:"tenant".into(),kind:"model_publication".into(),actor_id:"actor".into(),fields:ResourceWrite{code:"model_receipt".into(),name:"actual model publication fixture".into(),description:None,parent_id:None,status:"published".into(),properties:json!({"model_version":2,"receipt":{"physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":table.columns.iter().map(|column|json!({"column_name":column.name,"data_type":column.data_type,"is_nullable":if column.nullable{"YES"}else{"NO"}})).collect::<Vec<_>>()}}),references:vec![ResourceReference{field:"model_id".into(),target_kind:"data_model".into(),target_id:"model".into()}]}}).await.unwrap();
    create(&resources,"atomic_metric","revenue","draft",json!({"process_id":"process","model_id":"model","metadata_object_id":"metadata","expression":"SUM(amount)","grain":"订单主键","grain_columns":["id"],"time_column":"event_time","time_timezone":"Asia/Shanghai","dimensions":["region"]})).await;
    let roles = Arc::new(SqliteRoleRepo::new(pool.clone()));
    let auth = AuthService::new(
        Arc::new(SqliteTenantRepo::new(pool.clone())),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        roles.clone(),
        sa_auth::test_manager(roles),
    );
    let token = auth.issue_session_for_user("actor").await.unwrap();
    let actor = auth.authenticate(&token).await.unwrap();
    let metadata = Arc::new(SqliteMetadataCatalogRepo::new(pool.clone()));
    let jobs = Arc::new(SqliteProductJobRepo::new(pool.clone()));
    let access = QueryAccess {
        tenant_id: Some("tenant".into()),
    };
    let guard = Arc::new(
        QueryAuthorizationService::new(
            Arc::new(SqliteDataAccessRepo::new(pool.clone())),
            metadata.clone(),
            access.clone(),
        )
        .with_job_repo(jobs.clone())
        .with_resource_repo(resources.clone()),
    );
    let service = MetricsService::new(resources.clone(), jobs, metadata, auth, access)
        .with_query_authorization(guard);
    Fixture {
        pool,
        resources,
        service,
        actor,
    }
}
fn request(version: i64, key: &str) -> MetricRunRequest {
    MetricRunRequest {
        expected_version: version,
        idempotency_key: key.into(),
        window_start: Some("2026-09-01T00:00:00Z".parse().unwrap()),
        window_end: Some("2026-10-01T00:00:00Z".parse().unwrap()),
        as_of: None,
        row_limit: 10,
    }
}

#[tokio::test]
async fn sa_authenticated_publication_freezes_true_dependencies_and_enqueues_real_durable_job() {
    let fixture = setup().await;
    assert!(
        fixture
            .service
            .enqueue(
                &fixture.actor,
                "atomic_metric",
                "revenue",
                request(1, "draft-job")
            )
            .await
            .is_err()
    );
    let published = fixture
        .service
        .publish(&fixture.actor, "atomic_metric", "revenue", 1)
        .await
        .unwrap();
    assert_eq!(published["resource"]["status"], "published");
    assert_eq!(published["executed"], false);
    let queued = fixture
        .service
        .enqueue(
            &fixture.actor,
            "atomic_metric",
            "revenue",
            request(2, "real-job"),
        )
        .await
        .unwrap();
    assert!(queued.created);
    assert_eq!(queued.job.status, ProductJobStatus::Queued);
    assert!(queued.job.nodes[0].result.is_none());
    assert!(
        prepare_claimed(&queued.job).unwrap()[0]
            .1
            .contains("SUM(\"amount\")")
    );
    let replay = fixture
        .service
        .enqueue(
            &fixture.actor,
            "atomic_metric",
            "revenue",
            request(2, "real-job"),
        )
        .await
        .unwrap();
    assert!(!replay.created);
    assert_eq!(replay.job.id, queued.job.id);
    fixture
        .service
        .withdraw(&fixture.actor, "atomic_metric", "revenue", 2)
        .await
        .unwrap();
    assert!(
        verify_current(
            fixture.resources.as_ref(),
            &SqliteMetadataCatalogRepo::new(fixture.pool.clone()),
            None,
            &queued.job
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn physical_namespace_metadata_forgery_and_changed_model_are_rejected() {
    let fixture = setup().await;
    sqlx::query("UPDATE metadata_catalog SET discovery_status='missing' WHERE tenant_id='tenant'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(
        fixture
            .service
            .publish(&fixture.actor, "atomic_metric", "revenue", 1)
            .await
            .is_err()
    );
    sqlx::query("UPDATE metadata_catalog SET discovery_status='present' WHERE tenant_id='tenant'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    let receipt = fixture
        .resources
        .get("tenant", "model_publication", "model-receipt")
        .await
        .unwrap()
        .unwrap();
    let mut properties = receipt.properties.clone();
    properties["receipt"]["physical_binding"]["table_catalog"] = json!("other_catalog");
    fixture
        .resources
        .update(
            "tenant",
            "model_publication",
            "model-receipt",
            &UpdateResourceParams {
                expected_version: receipt.version,
                actor_id: "actor".into(),
                fields: ResourceWrite {
                    code: receipt.code,
                    name: receipt.name,
                    description: None,
                    parent_id: None,
                    status: receipt.status,
                    properties,
                    references: receipt.references,
                },
            },
        )
        .await
        .unwrap();
    assert!(
        fixture
            .service
            .publish(&fixture.actor, "atomic_metric", "revenue", 1)
            .await
            .is_err()
    );
    let other = AuthIdentity {
        sub: fixture.actor.sub.clone(),
        tenant_id: "other".into(),
        username: fixture.actor.username.clone(),
    };
    assert!(
        fixture
            .service
            .validate_saved(&other, "atomic_metric", "revenue", 1)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn derived_and_composite_freeze_period_modifier_units_and_dependency_withdrawals() {
    let fixture = setup().await;
    fixture
        .service
        .publish(&fixture.actor, "atomic_metric", "revenue", 1)
        .await
        .unwrap();
    create(
        &fixture.resources,
        "metric_period",
        "month",
        "draft",
        json!({"unit":"month","length":1,"timezone":"Asia/Shanghai"}),
    )
    .await;
    create(
        &fixture.resources,
        "metric_modifier",
        "east",
        "draft",
        json!({"metadata_object_id":"metadata","dimension":"region","condition":"region = 'east'"}),
    )
    .await;
    fixture
        .service
        .publish(&fixture.actor, "metric_period", "month", 1)
        .await
        .unwrap();
    fixture
        .service
        .publish(&fixture.actor, "metric_modifier", "east", 1)
        .await
        .unwrap();
    create(&fixture.resources,"derived_metric","monthly","draft",json!({"atomic_metric_id":"revenue","modifier_id":"east","period_id":"month","dimensions":["region"]})).await;
    fixture
        .service
        .publish(&fixture.actor, "derived_metric", "monthly", 1)
        .await
        .unwrap();
    create(
        &fixture.resources,
        "composite_metric",
        "double_monthly",
        "draft",
        json!({"metric_ids":["monthly"],"expression":"monthly * 2"}),
    )
    .await;
    fixture
        .service
        .publish(&fixture.actor, "composite_metric", "double_monthly", 1)
        .await
        .unwrap();
    let request = MetricRunRequest {
        expected_version: 2,
        idempotency_key: "derived-compute".into(),
        window_start: None,
        window_end: None,
        as_of: Some("2026-10-09T00:00:00Z".parse().unwrap()),
        row_limit: 10,
    };
    let queued = fixture
        .service
        .enqueue(
            &fixture.actor,
            "composite_metric",
            "double_monthly",
            request,
        )
        .await
        .unwrap();
    let sql = &prepare_claimed(&queued.job).unwrap()[0].1;
    assert!(sql.contains("CASE WHEN"));
    assert!(sql.contains("2026-08-31T16:00:00+00:00"));
    fixture
        .service
        .withdraw(&fixture.actor, "metric_modifier", "east", 2)
        .await
        .unwrap();
    assert!(
        verify_current(
            fixture.resources.as_ref(),
            &SqliteMetadataCatalogRepo::new(fixture.pool.clone()),
            None,
            &queued.job
        )
        .await
        .is_err()
    );
}
