use chrono::Utc;
use dts_backend::{
    repo::assets::{validation, *},
    *,
};
use dts_backend_sqlite::SqliteProductResourceRepo;
use serde_json::{Value, json};
use sqlx::SqlitePool;

#[path = "../data_access_support/mod.rs"]
pub mod access;

pub async fn setup() -> (SqlitePool, SqliteProductResourceRepo) {
    let (pool, _, _) = access::setup().await;
    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../../migrations/014_assets.sql"))
            .execute(&pool)
            .await
            .expect("repeatable assets migration");
    }
    for permission in [
        "assets:read",
        "assets:write",
        "assets:execute",
        "metadata:read",
        "models:read",
        "metrics:read",
        "standards:read",
    ] {
        access::permission(
            &pool,
            "a-requester",
            &permission.replace(':', "_"),
            permission,
            false,
        )
        .await;
    }
    let repo = SqliteProductResourceRepo::new(pool.clone());
    (pool, repo)
}

pub fn save(id: &str, kind: &str, properties: Value) -> SaveAssetParams {
    SaveAssetParams {
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        id: id.into(),
        kind: kind.into(),
        expected_version: None,
        fields: ResourceWrite {
            code: id.replace('-', "_"),
            name: id.into(),
            description: None,
            parent_id: None,
            status: "draft".into(),
            references: validation::references(kind, &properties).expect("references"),
            properties,
        },
    }
}
pub fn entry(id: &str, kind: &str, source: &str) -> SaveAssetParams {
    save(
        id,
        "asset_entry",
        json!({"object_kind":kind,"object_id":source,"owner_id":"a-approver","tags":["catalog"]}),
    )
}
pub fn scope(resource: &ProductResource) -> AssetScope {
    AssetScope {
        tenant_id: resource.tenant_id.clone(),
        actor_id: "a-requester".into(),
        id: resource.id.clone(),
        expected_version: resource.version,
    }
}

pub async fn transition(
    repo: &SqliteProductResourceRepo,
    resource: &ProductResource,
    action: AssetTransition,
) -> Result<AssetView, BackendError> {
    let validated_source = if action == AssetTransition::Withdraw {
        None
    } else {
        let kind = resource.properties["object_kind"].as_str().expect("kind");
        let id = resource.properties["object_id"].as_str().expect("id");
        let source = repo
            .get(&resource.tenant_id, kind, id)
            .await
            .expect("get source")
            .expect("source");
        Some(AssetSourceCheck {
            kind: kind.into(),
            id: id.into(),
            version: source.version,
            definition_hash: validation::hash(&source.properties).expect("hash"),
        })
    };
    repo.transition(&TransitionAssetParams {
        scope: scope(resource),
        action,
        validated_source,
    })
    .await
}

pub async fn publish(repo: &SqliteProductResourceRepo, id: &str) -> AssetView {
    let draft = repo
        .save(&entry(id, "metadata_object", "metadata-a"))
        .await
        .expect("draft");
    let registered = transition(repo, &draft, AssetTransition::Register)
        .await
        .expect("registered");
    transition(repo, &registered.resource, AssetTransition::Publish)
        .await
        .expect("published")
}

pub fn read(id: &str) -> AssetReadParams {
    AssetReadParams {
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        id: id.into(),
    }
}

pub async fn source(pool: &SqlitePool, id: &str, kind: &str, properties: Value) {
    sqlx::query("INSERT INTO product_resource(id,tenant_id,kind,code,name,status,properties,version,created_by,created_at,updated_at)
        VALUES(?1,'a',?2,?1,?1,'draft',?3,1,'a-approver',?4,?4)")
        .bind(id).bind(kind).bind(properties.to_string()).bind(Utc::now().to_rfc3339()).execute(pool).await.expect("source");
}

pub fn usage(id: &str) -> AssetUsageParams {
    AssetUsageParams {
        tenant_id: "a".into(),
        actor_id: "a-requester".into(),
        request: AssetUsageRequest {
            asset_id: id.into(),
            period_start: Utc::now() - chrono::Duration::hours(1),
            period_end: Utc::now(),
        },
    }
}

pub async fn audit(
    pool: &SqlitePool,
    id: &str,
    actor: &str,
    metadata: &str,
    outcome: &str,
    time: chrono::DateTime<Utc>,
) {
    sqlx::query("INSERT INTO data_access_audit(id,tenant_id,actor_id,action,outcome,evidence,created_at) VALUES(?1,'a',?2,'query',?3,?4,?5)")
        .bind(id).bind(actor).bind(outcome).bind(json!({"metadata_ids":[metadata],"query_hash":"recorded_hash","grant_ids":[],"columns":["id"],"all_data":true}).to_string())
        .bind(time.to_rfc3339()).execute(pool).await.expect("audit");
}
