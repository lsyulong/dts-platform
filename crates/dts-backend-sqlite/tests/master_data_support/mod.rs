// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{repo::master_data::*, repo::product_resource::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMasterDataRepo, SqliteMigrationRunner};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn setup() -> (SqlitePool, SqliteMasterDataRepo) {
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
        sqlx::raw_sql(include_str!("../../migrations/009_master_data.sql"))
            .execute(&pool)
            .await
            .expect("master migration");
    }
    for tenant in ["a", "b"] {
        sqlx::query("INSERT INTO tenant (id,name,slug) VALUES (?1,?1,?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("tenant");
        for role in ["owner", "approver", "other"] {
            let id = format!("{tenant}-{role}");
            sqlx::query("INSERT INTO \"user\" (id,tenant_id,username,email,password_hash) VALUES (?1,?2,?1,?1,'hash')")
                .bind(id).bind(tenant).execute(&pool).await.expect("actor");
        }
    }
    sqlx::query("INSERT INTO role (id,tenant_id,name) VALUES ('approval','a','approval')")
        .execute(&pool)
        .await
        .expect("role");
    sqlx::query("INSERT INTO role_permission (role_id,permission) VALUES ('approval','master_data:approve')").execute(&pool).await.expect("permission");
    sqlx::query("INSERT INTO user_role (user_id,role_id) VALUES ('a-approver','approval')")
        .execute(&pool)
        .await
        .expect("approver membership");
    let repo = SqliteMasterDataRepo::new(pool.clone());
    let model=repo.save_model(&write("model",None,json!({"columns":[{"name":"id","data_type":"INTEGER","nullable":false,"description":"ID"},
        {"name":"name","data_type":"VARCHAR","nullable":false,"description":"Name"}],"key_fields":["id"]}))).await.expect("model");
    repo.publish_model(&scope(&model, "a-owner"))
        .await
        .expect("freeze");
    (pool, repo)
}

pub fn write(id: &str, version: Option<i64>, properties: Value) -> SaveMasterResourceParams {
    SaveMasterResourceParams {
        tenant_id: "a".into(),
        id: id.into(),
        actor_id: "a-owner".into(),
        expected_version: version,
        fields: ResourceWrite {
            code: id.replace('-', "_"),
            name: id.into(),
            description: None,
            parent_id: None,
            status: "draft".into(),
            properties,
            references: vec![],
        },
    }
}

pub fn record(
    id: &str,
    business: &str,
    key: i64,
    name: &str,
    version: Option<i64>,
) -> SaveMasterResourceParams {
    write(
        id,
        version,
        json!({"model_id":"model","business_key":business,"values":{"id":key,"name":name}}),
    )
}

pub fn scope(resource: &ProductResource, actor: &str) -> MasterScope {
    MasterScope {
        tenant_id: resource.tenant_id.clone(),
        id: resource.id.clone(),
        actor_id: actor.into(),
        expected_version: resource.version,
    }
}

pub async fn submitted(repo: &SqliteMasterDataRepo, id: &str) -> ProductResource {
    let record = repo
        .save_record(&record(id, id, 1, "original", None))
        .await
        .expect("record");
    repo.transition_record(&MasterRecordActionParams {
        scope: scope(&record, "a-owner"),
        action: MasterRecordAction::Submit,
        reason: "ready".into(),
    })
    .await
    .expect("submit")
}

pub async fn rule(
    repo: &SqliteMasterDataRepo,
    id: &str,
    width: u32,
    start: i64,
) -> ProductResource {
    repo.save_rule(&write(
        id,
        None,
        json!({"model_id":"model","prefix":"M","width":width,"start_value":start}),
    ))
    .await
    .expect("rule")
}
