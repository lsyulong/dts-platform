use dts_backend::{
    ProductResource, ProductResourceRepo,
    repo::{product_job::*, runtime_configuration::*},
};
use dts_backend_sqlite::{
    SqliteProductJobRepo, SqliteProductResourceRepo, SqliteRuntimeConfigurationRepo,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;

pub async fn setup() -> (
    SqlitePool,
    SqliteRuntimeConfigurationRepo,
    SqliteProductJobRepo,
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
        include_str!("../../migrations/004_product_external_refs.sql"),
        include_str!("../../migrations/006_product_jobs.sql"),
        include_str!("../../migrations/024_runtime_configuration.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('a','A','a'),('b','B','b'); INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor','a','actor','a@test','hash'),('reader','a','reader','r@test','hash'),('other','b','other','b@test','hash'); INSERT INTO role(id,tenant_id,name) VALUES('developer','a','developer'),('reader-role','a','reader'); INSERT INTO user_role(user_id,role_id) VALUES('actor','developer'),('reader','reader-role'); INSERT INTO role_permission(role_id,permission) VALUES('developer','development:read'),('developer','development:write'),('developer','development:execute'),('reader-role','development:read');").execute(&pool).await.unwrap();
    insert(
        &pool,
        "development_config",
        "cfg",
        json!({"scope":"shared","variables":{},"concurrency":1,"timeout_seconds":1}),
    )
    .await;
    for id in ["one", "two", "three"] {
        insert(
            &pool,
            "sql_document",
            id,
            json!({"sql":"SELECT 1 AS value","configuration_id":"cfg"}),
        )
        .await;
    }
    (
        pool.clone(),
        SqliteRuntimeConfigurationRepo::new(pool.clone()),
        SqliteProductJobRepo::new(pool),
    )
}
pub async fn insert(pool: &SqlitePool, kind: &str, id: &str, properties: Value) {
    let time = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES(?1,'a',?2,?1,?1,'draft',?3,1,'actor',?4,?4)").bind(id).bind(kind).bind(properties.to_string()).bind(time).execute(pool).await.unwrap();
}
pub fn scope() -> RuntimeConfigurationScope {
    RuntimeConfigurationScope {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
    }
}
pub fn lookup() -> RuntimeConfigurationLookup {
    RuntimeConfigurationLookup {
        scope: scope(),
        configuration_id: "cfg".into(),
    }
}
pub fn source_lookup(id: &str, version: i64, access: RuntimeSourceAccess) -> RuntimeSourceLookup {
    RuntimeSourceLookup {
        scope: scope(),
        source_kind: "sql_document".into(),
        source_id: id.into(),
        source_version: version,
        access,
    }
}
pub async fn resource(pool: &SqlitePool, kind: &str, id: &str) -> ProductResource {
    SqliteProductResourceRepo::new(pool.clone())
        .get("a", kind, id)
        .await
        .unwrap()
        .unwrap()
}
pub async fn publish_config(
    pool: &SqlitePool,
    repo: &SqliteRuntimeConfigurationRepo,
) -> RuntimeConfigurationChange {
    let cfg = resource(pool, "development_config", "cfg").await;
    repo.publish(&PublishRuntimeConfigurationParams {
        lookup: lookup(),
        expected_version: cfg.version,
        reason: "Actual configuration release".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        validation: Some(ValidatedRuntimePolicy::from_validated(&cfg, None).unwrap()),
    })
    .await
    .unwrap()
}
pub async fn publish_source(
    pool: &SqlitePool,
    repo: &SqliteRuntimeConfigurationRepo,
    id: &str,
) -> ProductResource {
    let src = resource(pool, "sql_document", id).await;
    let input = repo
        .current_source(&source_lookup(
            id,
            src.version,
            RuntimeSourceAccess::Publish,
        ))
        .await
        .unwrap();
    repo.publish_source(&PublishRuntimeSourceParams {
        source: source_lookup(id, src.version, RuntimeSourceAccess::Publish),
        validation: ValidatedRuntimeSource::from_validated(&src).unwrap(),
        evidence: input.evidence,
    })
    .await
    .unwrap()
}
pub fn enqueue(
    src: &ProductResource,
    evidence: RuntimeConfigurationEvidence,
) -> EnqueueProductJobParams {
    EnqueueProductJobParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        source_kind: src.kind.clone(),
        source_id: src.id.clone(),
        source_version: src.version,
        action: "execute".into(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        definition_snapshot: json!({"format_version":1,"runtime_configuration":evidence}),
        parameters: json!({}),
        can_retry: true,
        nodes: vec![ProductJobNodeDefinition {
            id: "query".into(),
            name: "query".into(),
            definition: json!({"sql":"SELECT 1 AS value"}),
            depends_on: vec![],
        }],
    }
}
pub fn claim(worker: &str) -> ClaimProductJobParams {
    ClaimProductJobParams {
        tenant_id: "a".into(),
        worker_id: worker.into(),
        source_kinds: vec!["sql_document".into()],
        lease_seconds: 30,
    }
}
pub fn lease(job: &ProductJob) -> ProductJobLease {
    ProductJobLease {
        worker_id: job.lease_owner.clone().unwrap(),
        fence: job.lease_fence,
    }
}
pub async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
