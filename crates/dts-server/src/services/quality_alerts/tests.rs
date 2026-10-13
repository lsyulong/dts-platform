use super::*;
use dts_backend::{
    CreateResourceParams, ResourceWrite,
    repo::metadata_catalog::{MetadataColumn, MetadataTable},
};
use dts_backend_sqlite::{
    SqliteDataAccessRepo, SqliteMetadataCatalogRepo, SqliteProductJobRepo,
    SqliteProductResourceRepo, SqliteQualityAlertRepo, SqliteRoleRepo, SqliteTenantRepo,
    SqliteUserRepo,
};
use serde_json::json;

struct Fixture {
    pool: sqlx::SqlitePool,
    service: QualityAlertsService,
    actor: AuthIdentity,
    resources: Arc<SqliteProductResourceRepo>,
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
        include_str!("../../../../dts-backend-sqlite/migrations/001_initial_schema.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/002_product_resources.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/004_product_external_refs.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/006_product_jobs.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/011_metadata_catalog.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/012_data_access.sql"),
        include_str!("../../../../dts-backend-sqlite/migrations/021_quality_alerts.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('tenant','Tenant','tenant'); INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor','tenant','actor','actor@test','hash'); INSERT INTO role(id,tenant_id,name,is_system) VALUES('role','tenant','tenant_admin',1); INSERT INTO user_role(user_id,role_id) VALUES('actor','role'); INSERT INTO role_permission(role_id,permission) VALUES('role','quality:read'),('role','quality:write'),('role','data_security:all_data');").execute(&pool).await.unwrap();
    let resources = Arc::new(SqliteProductResourceRepo::new(pool.clone()));
    let table = MetadataTable {
        table_catalog: "lake".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![MetadataColumn {
            name: "id".into(),
            data_type: "BIGINT".into(),
            nullable: false,
            ordinal_position: 1,
        }],
    };
    for (kind, id, properties) in [
        (
            "metadata_object",
            "metadata",
            table.properties("tenant").unwrap(),
        ),
        (
            "quality_object",
            "object",
            json!({"metadata_object_id":"metadata","columns":["id"]}),
        ),
        (
            "quality_monitor",
            "monitor",
            json!({"object_id":"object","failure_threshold":0.1,"window_hours":1,"recipient_ids":["actor"]}),
        ),
    ] {
        resources
            .create(&CreateResourceParams {
                id: id.into(),
                tenant_id: "tenant".into(),
                kind: kind.into(),
                actor_id: "actor".into(),
                fields: ResourceWrite {
                    code: id.into(),
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
                    references: vec![],
                },
            })
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at) VALUES('tenant','swanlake','lake','main','orders','metadata',1,'BASE TABLE',?1,?2,'present','collect',?3)").bind(json!(table.columns).to_string()).bind(table.fingerprint().unwrap()).bind(chrono::Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
    let roles = Arc::new(SqliteRoleRepo::new(pool.clone()));
    let auth = AuthService::new(
        Arc::new(SqliteTenantRepo::new(pool.clone())),
        Arc::new(SqliteUserRepo::new(pool.clone())),
        roles.clone(),
        crate::services::sa_auth::test_manager(roles),
    );
    let token = auth.issue_session_for_user("actor").await.unwrap();
    let actor = auth.authenticate(&token).await.unwrap();
    let metadata = Arc::new(SqliteMetadataCatalogRepo::new(pool.clone()));
    let jobs = Arc::new(SqliteProductJobRepo::new(pool.clone()));
    let guard = Arc::new(
        QueryAuthorizationService::new(
            Arc::new(SqliteDataAccessRepo::new(pool.clone())),
            metadata.clone(),
            crate::api::query::QueryAccess {
                tenant_id: Some("tenant".into()),
            },
        )
        .with_job_repo(jobs.clone())
        .with_resource_repo(resources.clone()),
    );
    let service = QualityAlertsService::new(
        resources.clone(),
        jobs,
        metadata,
        guard,
        auth,
        Arc::new(SqliteQualityAlertRepo::new(pool.clone())),
    );
    Fixture {
        pool,
        service,
        actor,
        resources,
    }
}
fn request(key: &str) -> QualityEvaluateRequest {
    QualityEvaluateRequest {
        expected_version: 1,
        idempotency_key: key.into(),
    }
}

#[tokio::test]
async fn actual_empty_window_persists_unavailable_and_replay_never_fabricates_an_alert() {
    let fixture = setup().await;
    let result = fixture
        .service
        .evaluate(&fixture.actor, "monitor", request("manual-evaluate"))
        .await
        .unwrap();
    assert_eq!(result.observation.state, "unavailable");
    assert_eq!(result.observation.reason, "no_results");
    assert!(result.observation.summary["threshold_exceeded"].is_null());
    assert!(result.alert.is_none());
    assert!(result.observation.run_ids.is_empty());
    let replay = fixture
        .service
        .evaluate(&fixture.actor, "monitor", request("manual-evaluate"))
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.observation.id, result.observation.id);
    let page = fixture
        .service
        .list_observations(&fixture.actor, "monitor", 10, 0)
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert!(
        fixture
            .service
            .list_notices(&fixture.actor, true, 10, 0)
            .await
            .unwrap()
            .data
            .is_empty()
    );
    let serialized = serde_json::to_value(page).unwrap();
    assert!(serialized["data"][0].get("snapshot").is_none());
    assert!(serialized.to_string().find("engine_binding").is_none());
    sqlx::query("DELETE FROM role_permission WHERE permission='data_security:all_data'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .list_observations(&fixture.actor, "monitor", 10, 0)
            .await,
        Err(AppError::Forbidden(_))
    ));
}

#[tokio::test]
async fn current_actor_metadata_and_monitor_version_are_enforced_before_persistence() {
    let fixture = setup().await;
    let mut wrong = request("wrong-version");
    wrong.expected_version = 2;
    assert!(matches!(
        fixture
            .service
            .evaluate(&fixture.actor, "monitor", wrong)
            .await,
        Err(AppError::Conflict(_))
    ));
    sqlx::query("UPDATE metadata_catalog SET discovery_status='missing'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(
        fixture
            .service
            .evaluate(&fixture.actor, "monitor", request("missing-proof"))
            .await
            .is_err()
    );
    sqlx::query("UPDATE metadata_catalog SET discovery_status='present'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_permission WHERE permission='quality:write'")
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert!(
        fixture
            .service
            .evaluate(&fixture.actor, "monitor", request("no-write"))
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quality_observation")
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        fixture
            .resources
            .get("other", "quality_monitor", "monitor")
            .await
            .unwrap()
            .is_none()
    );
}

#[test]
fn client_clock_actor_and_fake_status_are_not_accepted() {
    for body in [
        json!({"expected_version":1,"idempotency_key":"key","observed_at":"2030-01-01T00:00:00Z"}),
        json!({"expected_version":1,"idempotency_key":"key","actor_id":"other"}),
    ] {
        assert!(serde_json::from_value::<QualityEvaluateRequest>(body).is_err());
    }
    assert!(serde_json::from_value::<QualityAckRequest>(json!({"expected_version":1,"idempotency_key":"key","reason":"checking","status":"resolved"})).is_err());
}
