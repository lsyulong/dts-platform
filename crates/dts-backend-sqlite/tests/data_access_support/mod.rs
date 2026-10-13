use chrono::{Duration, Utc};
use dts_backend::{
    ProductResource,
    repo::{data_access::*, metadata_catalog::*},
};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteDataAccessRepo, SqliteMigrationRunner};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn setup() -> (SqlitePool, SqliteDataAccessRepo, DataAccessTarget) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("SQLite");
    SqliteMigrationRunner::new(pool.clone())
        .run_migrations()
        .await
        .expect("migrations");
    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../../migrations/012_data_access.sql"))
            .execute(&pool)
            .await
            .expect("repeatable access migration");
    }
    for tenant in ["a", "b"] {
        sqlx::query("INSERT INTO tenant(id,name,slug) VALUES(?1,?1,?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("tenant");
        for actor in ["requester", "approver", "other"] {
            sqlx::query("INSERT INTO \"user\"(id,tenant_id,username,email,password_hash) VALUES(?1,?2,?1,?1,'hash')")
                .bind(format!("{tenant}-{actor}")).bind(tenant).execute(&pool).await.expect("actor");
        }
    }
    permission(
        &pool,
        "a-approver",
        "approval",
        "data_security:approve",
        false,
    )
    .await;
    let table = MetadataTable {
        table_catalog: "warehouse".into(),
        schema_name: "main".into(),
        table_name: "orders".into(),
        table_type: "BASE TABLE".into(),
        columns: vec![
            MetadataColumn {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                ordinal_position: 1,
            },
            MetadataColumn {
                name: "amount".into(),
                data_type: "DECIMAL(18,2)".into(),
                nullable: true,
                ordinal_position: 2,
            },
        ],
    };
    let fingerprint = table.fingerprint().expect("fingerprint");
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at)
        VALUES('metadata-a','a','metadata_object','orders','Orders','published',?1,1,'a-approver',?2,?2)")
        .bind(table.properties("a").expect("properties").to_string()).bind(Utc::now().to_rfc3339()).execute(&pool).await.expect("real metadata resource");
    sqlx::query("INSERT INTO metadata_catalog(tenant_id,engine,table_catalog,schema_name,table_name,object_id,object_version,table_type,columns_snapshot,source_fingerprint,discovery_status,last_collection_id,last_observed_at)
        VALUES('a','swanlake','warehouse','main','orders','metadata-a',1,'BASE TABLE',?1,?2,'present','collection-a',?3)")
        .bind(json!(table.columns).to_string()).bind(&fingerprint).bind(Utc::now().to_rfc3339()).execute(&pool).await.expect("collector proof fixture");
    let target = DataAccessTarget {
        metadata_id: "metadata-a".into(),
        binding: MetadataEngineBinding {
            engine: "swanlake".into(),
            tenant_id: "a".into(),
            object_version: 1,
            table_catalog: table.table_catalog,
            schema_name: table.schema_name,
            table_name: table.table_name,
            source_fingerprint: fingerprint,
        },
        columns: table.columns,
    };
    let repo = SqliteDataAccessRepo::new(pool.clone());
    (pool, repo, target)
}
pub async fn permission(
    pool: &SqlitePool,
    actor: &str,
    name: &str,
    permission: &str,
    system: bool,
) {
    let id = format!("{actor}-{name}");
    sqlx::query("INSERT INTO role(id,tenant_id,name,is_system) VALUES(?1,'a',?2,?3)")
        .bind(&id)
        .bind(name)
        .bind(system)
        .execute(pool)
        .await
        .expect("role");
    sqlx::query("INSERT INTO role_permission(role_id,permission) VALUES(?1,?2)")
        .bind(&id)
        .bind(permission)
        .execute(pool)
        .await
        .expect("permission");
    sqlx::query("INSERT INTO user_role(user_id,role_id) VALUES(?1,?2)")
        .bind(actor)
        .bind(id)
        .execute(pool)
        .await
        .expect("membership");
}
pub fn save(id: &str, target: &DataAccessTarget) -> SaveDataAccessRequestParams {
    SaveDataAccessRequestParams {
        id: id.into(),
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        expected_version: None,
        code: id.replace('-', "_"),
        name: id.into(),
        description: None,
        target: target.clone(),
        requested_columns: vec!["id".into()],
        purpose: "Only a private business purpose; do not copy to audit".into(),
        expires_at: Utc::now() + Duration::hours(4),
    }
}
pub fn decision(
    resource: &ProductResource,
    actor: &str,
    action: DataAccessDecision,
    key: &str,
) -> DecideDataAccessParams {
    DecideDataAccessParams {
        tenant_id: resource.tenant_id.clone(),
        request_id: resource.id.clone(),
        expected_version: resource.version,
        actor_id: actor.into(),
        decision: action,
        reason: if action.needs_approver() {
            "Validated business requirement".into()
        } else {
            String::new()
        },
        expires_at: None,
        idempotency_key: key.into(),
    }
}
pub async fn approved(
    repo: &SqliteDataAccessRepo,
    target: &DataAccessTarget,
    id: &str,
) -> DataAccessDecisionResult {
    let draft = repo.save_request(&save(id, target)).await.expect("draft");
    let pending = repo
        .decide(&decision(
            &draft,
            "a-requester",
            DataAccessDecision::Submit,
            &format!("{id}-submit"),
        ))
        .await
        .expect("pending");
    repo.decide(&decision(
        &pending.request,
        "a-approver",
        DataAccessDecision::Approve,
        &format!("{id}-approve"),
    ))
    .await
    .expect("approved")
}
