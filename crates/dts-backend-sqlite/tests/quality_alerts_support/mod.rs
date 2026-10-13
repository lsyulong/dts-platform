use chrono::{Duration, Utc};
use dts_backend::{
    repo::metadata_catalog::{MetadataColumn, MetadataTable},
    repo::quality_alerts::*,
};
use dts_backend_sqlite::SqliteQualityAlertRepo;
use serde_json::{Value, json};
use sqlx::SqlitePool;

pub async fn setup() -> (
    SqlitePool,
    SqliteQualityAlertRepo,
    RecordQualityEvaluationParams,
) {
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
        include_str!("../../migrations/001_initial_schema.sql"),
        include_str!("../../migrations/002_product_resources.sql"),
        include_str!("../../migrations/006_product_jobs.sql"),
        include_str!("../../migrations/011_metadata_catalog.sql"),
        include_str!("../../migrations/021_quality_alerts.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('a','A','a'),('b','B','b'); INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor','a','actor','actor@test','hash'),('reader','a','reader','reader@test','hash'),('other','b','other','other@test','hash'); INSERT INTO role(id,tenant_id,name) VALUES('role-a','a','quality'); INSERT INTO user_role(user_id,role_id) VALUES('actor','role-a'); INSERT INTO role_permission(role_id,permission) VALUES('role-a','quality:write'),('role-a','quality:read');").execute(&pool).await.unwrap();
    let end = Utc::now();
    let event = end - Duration::seconds(10);
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
    let props = [
        (
            "quality_monitor",
            "monitor",
            json!({"object_id":"object","window_hours":1,"failure_threshold":0.1}),
        ),
        (
            "quality_object",
            "object",
            json!({"metadata_object_id":"metadata"}),
        ),
        (
            "metadata_object",
            "metadata",
            table.properties("a").unwrap(),
        ),
        (
            "quality_task",
            "task",
            json!({"object_id":"object","rule_ids":["rule"]}),
        ),
        ("quality_rule", "rule", json!({"object_id":"object"})),
    ];
    let mut resources = Vec::new();
    for (kind, id, properties) in props {
        sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES(?1,'a',?2,?1,?1,CASE WHEN ?2='metadata_object' THEN 'published' ELSE 'draft' END,?3,1,'actor',?4,?4)").bind(id).bind(kind).bind(properties.to_string()).bind(event.to_rfc3339()).execute(&pool).await.unwrap();
        resources.push(QualityResourceProof {
            kind: kind.into(),
            id: id.into(),
            version: 1,
            properties,
        });
    }
    let fingerprint = table.fingerprint().unwrap();
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at) VALUES('a','swanlake','lake','main','orders','metadata',1,'BASE TABLE',?1,?2,'present','collect',?3)").bind(json!(table.columns).to_string()).bind(&fingerprint).bind(event.to_rfc3339()).execute(&pool).await.unwrap();
    let snapshot = json!({"object":{"id":"object"}});
    let definition = json!({"rule_id":"rule"});
    let result = json!({"checked_rows":10,"failed_rows":3,"evaluation_scope":"full_table"});
    sqlx::query("INSERT INTO product_job(id,tenant_id,source_kind,source_id,source_version,actor_id,action,idempotency_key,definition_snapshot,parameters,status,can_retry,created_at,updated_at,started_at,finished_at) VALUES('job','a','quality_task','task',1,'actor','execute_quality','job-key',?1,'{}','succeeded',1,?2,?2,?2,?2)").bind(snapshot.to_string()).bind(event.to_rfc3339()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO product_job_node(tenant_id,job_id,node_id,name,definition,depends_on,status,result,started_at,finished_at) VALUES('a','job','rule','rule',?1,'[]','succeeded',?2,?3,?3)").bind(definition.to_string()).bind(result.to_string()).bind(event.to_rfc3339()).execute(&pool).await.unwrap();
    let jobs = vec![QualityJobProof {
        id: "job".into(),
        source_id: "task".into(),
        source_version: 1,
        status: "succeeded".into(),
        updated_at: event,
        event_time: event,
        definition_snapshot: snapshot,
        nodes: vec![QualityNodeProof {
            id: "rule".into(),
            status: "succeeded".into(),
            definition,
            result: Some(result),
        }],
    }];
    let observation = QualityObservation {
        id: "observation".into(),
        tenant_id: "a".into(),
        monitor_id: "monitor".into(),
        monitor_version: 1,
        object_id: "object".into(),
        actor_id: "actor".into(),
        period_start: end - Duration::hours(1),
        period_end: end,
        state: "threshold_exceeded".into(),
        reason: "threshold_exceeded".into(),
        summary: summary(3),
        snapshot: QualityObservationSnapshot {
            resources,
            metadata_id: "metadata".into(),
            metadata_version: 1,
            source_fingerprint: fingerprint,
            jobs,
        },
        created_at: end,
    };
    let repo = SqliteQualityAlertRepo::new(pool.clone());
    (
        pool,
        repo,
        RecordQualityEvaluationParams {
            observation,
            idempotency_key: "evaluate-key".into(),
            recipient_ids: vec!["reader".into()],
            notice_subject: "Orders quality alert".into(),
        },
    )
}
pub fn summary(failed: u64) -> Value {
    json!({"completed_runs":1,"failed_runs":0,"incomplete_runs":0,"successful_checks":1,"empty_checks":0,"execution_error_checks":0,"sampled_checks":0,"checked_rows":10,"failed_rows":failed,"failure_ratio":failed as f64/10.0,"threshold":0.1,"threshold_exceeded":failed>1,"window_complete":true})
}
pub async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = match table {
        "quality_observation" => "SELECT COUNT(*) FROM quality_observation",
        "quality_alert" => "SELECT COUNT(*) FROM quality_alert",
        "quality_alert_event" => "SELECT COUNT(*) FROM quality_alert_event",
        "quality_alert_notice" => "SELECT COUNT(*) FROM quality_alert_notice",
        _ => panic!("not a test table"),
    };
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}
