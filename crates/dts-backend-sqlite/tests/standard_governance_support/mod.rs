use dts_backend::{ProductResource, ProductResourceRepo, repo::standard_governance::*};
use dts_backend_sqlite::{SqliteProductResourceRepo, SqliteStandardGovernanceRepo};
use serde_json::{Value, json};
use sqlx::SqlitePool;

pub async fn setup() -> (SqlitePool, SqliteStandardGovernanceRepo) {
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
        include_str!("../../migrations/011_metadata_catalog.sql"),
        include_str!("../../migrations/022_standard_governance.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO tenant(id,name,slug) VALUES('a','A','a'),('b','B','b'); INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES('actor','a','actor','actor@test','hash'),('reader','a','reader','reader@test','hash'),('other','b','other','other@test','hash'); INSERT INTO role(id,tenant_id,name) VALUES('role-a','a','standards'); INSERT INTO user_role(user_id,role_id) VALUES('actor','role-a'); INSERT INTO role_permission(role_id,permission) VALUES('role-a','standards:write'),('role-a','standards:execute'),('role-a','standards:read'),('role-a','models:write'),('role-a','models:execute'),('role-a','models:read'),('role-a','metadata:read');").execute(&pool).await.unwrap();
    for (kind, id, properties) in [
        (
            "field_standard",
            "field",
            json!({"data_type":"BIGINT","nullable":false,"definition":"Order identifier"}),
        ),
        (
            "unit_standard",
            "unit",
            json!({"symbol":"kg","dimension":"mass","conversion_factor":1}),
        ),
        (
            "enum_standard",
            "enum",
            json!({"enum_values":[{"value":"A","label":"A"}]}),
        ),
        (
            "data_model",
            "model",
            json!({"layer_id":"layer","table_name":"orders","columns":[{"name":"id","data_type":"BIGINT","nullable":false}],"primary_key":["id"]}),
        ),
    ] {
        insert(&pool, kind, id, "draft", 1, properties).await;
    }
    (pool.clone(), SqliteStandardGovernanceRepo::new(pool))
}
pub async fn insert(
    pool: &SqlitePool,
    kind: &str,
    id: &str,
    status: &str,
    version: i64,
    properties: Value,
) {
    let time = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at) VALUES(?1,'a',?2,?1,?1,?3,?4,?5,'actor',?6,?6)").bind(id).bind(kind).bind(status).bind(properties.to_string()).bind(version).bind(time).execute(pool).await.unwrap();
}
pub async fn source(pool: &SqlitePool, kind: &str, id: &str) -> ProductResource {
    SqliteProductResourceRepo::new(pool.clone())
        .get("a", kind, id)
        .await
        .unwrap()
        .unwrap()
}
pub fn scope(kind: &str, id: &str, version: i64) -> StandardScope {
    StandardScope {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        kind: kind.into(),
        id: id.into(),
        expected_version: version,
    }
}
pub async fn publish(
    pool: &SqlitePool,
    repo: &SqliteStandardGovernanceRepo,
    kind: &str,
    id: &str,
    children: Vec<FrozenStandardRef>,
) -> StandardRevision {
    let before = source(pool, kind, id).await;
    repo.transition(&StandardTransitionParams {
        scope: scope(kind, id, before.version),
        command: StandardCommand::Publish {
            expected_properties: before.properties.clone(),
            normalized_properties: before.properties,
            children,
        },
    })
    .await
    .unwrap()
    .revision
    .unwrap()
}
pub async fn bind_params(
    pool: &SqlitePool,
    reference: FrozenStandardRef,
) -> BindModelStandardsParams {
    let before = source(pool, "data_model", "model").await;
    let mut properties = before.properties.clone();
    properties["columns"][0]["standard_id"] = json!(reference.id);
    properties["columns"][0]["standard_version"] = json!(reference.revision_version);
    properties["standard_coverage"] = json!("all_fields");
    BindModelStandardsParams {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        model_id: "model".into(),
        expected_model_version: before.version,
        expected_binding_version: None,
        expected_properties: before.properties,
        properties,
        coverage: StandardCoverage::AllFields,
        columns: vec![ModelColumnStandardBinding {
            column_name: "id".into(),
            field_standard: Some(reference),
            naming_standard: None,
        }],
        table_naming: None,
        upgrade: false,
    }
}
pub async fn prepared(
    pool: &SqlitePool,
    repo: &SqliteStandardGovernanceRepo,
) -> StandardModelPublicationClaimRecord {
    let before = source(pool, "data_model", "model").await;
    let bindings = repo
        .bindings(&StandardConsumerLookup {
            tenant_id: "a".into(),
            actor_id: "actor".into(),
            model_id: "model".into(),
        })
        .await
        .unwrap();
    let dependencies = if let Some(binding) = &bindings {
        binding
            .columns
            .iter()
            .flat_map(|b| [b.field_standard.clone(), b.naming_standard.clone()])
            .chain(std::iter::once(binding.table_naming.clone()))
            .flatten()
            .collect()
    } else {
        vec![]
    };
    repo.begin_model_publication(&BeginStandardModelPublicationParams {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        model_id: "model".into(),
        expected_model_version: before.version,
        expected_properties: before.properties,
        binding_version: bindings.as_ref().map(|b| b.binding_version),
        binding_hash: bindings.as_ref().map(|b| b.definition_hash.clone()),
        dependencies,
    })
    .await
    .unwrap()
}
pub fn receipt() -> Value {
    json!({"generated_ddl":"CREATE TABLE orders (id BIGINT NOT NULL, PRIMARY KEY(id));","physical_binding":{"table_catalog":"lake","schema_name":"main","table_name":"orders","table_type":"BASE TABLE"},"physical_columns":[{"column_name":"id","data_type":"BIGINT","is_nullable":"NO"}],"reconciled":true})
}
pub fn finish_params(
    claim: &StandardModelPublicationClaimRecord,
    outcome: StandardModelEngineOutcome,
) -> FinishStandardModelPublicationParams {
    FinishStandardModelPublicationParams {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        gate_id: claim.gate_id.clone(),
        expected_claim_version: claim.claim_version,
        outcome,
    }
}
pub async fn successful_model(
    pool: &SqlitePool,
    repo: &SqliteStandardGovernanceRepo,
) -> StandardModelPublicationResult {
    let reference = publish(pool, repo, "field_standard", "field", vec![])
        .await
        .reference();
    repo.bind_model(&bind_params(pool, reference).await)
        .await
        .unwrap();
    let claim = prepared(pool, repo).await;
    repo.finish_model_publication(&finish_params(
        &claim,
        StandardModelEngineOutcome::Published { receipt: receipt() },
    ))
    .await
    .unwrap()
}
pub async fn collected_metadata(pool: &SqlitePool) -> StandardInspectionParams {
    use dts_backend::repo::metadata_catalog::{MetadataColumn, MetadataTable};
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
    insert(
        pool,
        "metadata_object",
        "metadata",
        "published",
        1,
        table.properties("a").unwrap(),
    )
    .await;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at) VALUES('a','swanlake','lake','main','orders','metadata',1,'BASE TABLE',?1,?2,'present','collect',?3)").bind(json!(table.columns).to_string()).bind(table.fingerprint().unwrap()).bind(now).execute(pool).await.unwrap();
    StandardInspectionParams {
        tenant_id: "a".into(),
        actor_id: "actor".into(),
        model_id: "model".into(),
        expected_model_version: source(pool, "data_model", "model").await.version,
        expected_binding_version: 1,
        metadata_id: "metadata".into(),
        metadata_version: 1,
    }
}
pub fn inspection_result(snapshot: &StandardInspectionSnapshot) -> StandardInspectionResult {
    StandardInspectionResult {
        id: uuid::Uuid::new_v4().to_string(),
        model_id: snapshot.resource.id.clone(),
        model_version: snapshot.resource.version,
        binding_version: snapshot.bindings.binding_version,
        metadata_id: snapshot.metadata.id.clone(),
        metadata_version: snapshot.metadata.version,
        metadata_observed_at: snapshot.metadata_observed_at,
        source_fingerprint: snapshot.binding.source_fingerprint.clone(),
        evaluation_scope: "collected_schema".into(),
        state: "conformant".into(),
        issues: vec![],
        data_values_evaluated: false,
        unit_semantics_evaluated: false,
        dependency_versions: snapshot.revisions.iter().map(|r| r.reference()).collect(),
        created_at: chrono::Utc::now(),
    }
}
pub async fn count(pool: &SqlitePool, table: &str) -> i64 {
    assert!(
        [
            "standard_revision",
            "standard_model_binding",
            "standard_model_gate",
            "standard_inspection",
            "product_resource_audit"
        ]
        .contains(&table)
    );
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
