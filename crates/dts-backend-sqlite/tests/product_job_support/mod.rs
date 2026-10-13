// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use dts_backend::{repo::product_job::*, repo::product_resource::*};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteProductJobRepo, SqliteProductResourceRepo};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn setup() -> (SqlitePool, SqliteProductJobRepo) {
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
        sqlx::raw_sql(include_str!("../../migrations/006_product_jobs.sql"))
            .execute(&pool)
            .await
            .expect("job migrations");
    }
    for tenant in ["a", "b"] {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?1, ?1)")
            .bind(tenant)
            .execute(&pool)
            .await
            .expect("tenant");
        for suffix in ["owner", "other"] {
            let id = format!("{tenant}-{suffix}");
            sqlx::query("INSERT INTO \"user\" (id, tenant_id, username, email, password_hash) VALUES (?1, ?2, ?1, ?1, 'hash')")
                .bind(&id).bind(tenant).execute(&pool).await.expect("actor");
        }
        for (suffix, kind) in [
            ("sql", "sql_document"),
            ("workflow", "development_workflow"),
        ] {
            SqliteProductResourceRepo::new(pool.clone())
                .create(&CreateResourceParams {
                    id: format!("{tenant}-{suffix}"),
                    tenant_id: tenant.into(),
                    kind: kind.into(),
                    actor_id: format!("{tenant}-owner"),
                    fields: ResourceWrite {
                        code: suffix.into(),
                        name: suffix.into(),
                        description: None,
                        parent_id: None,
                        status: "published".into(),
                        properties: json!({"sql": "SELECT 1"}),
                        references: vec![],
                    },
                })
                .await
                .expect("source");
        }
    }
    let repo = SqliteProductJobRepo::new(pool.clone());
    (pool, repo)
}

pub fn request(id: &str) -> EnqueueProductJobParams {
    EnqueueProductJobParams {
        id: id.into(),
        tenant_id: "a".into(),
        source_kind: "sql_document".into(),
        source_id: "a-sql".into(),
        source_version: 1,
        actor_id: "a-owner".into(),
        action: "execute".into(),
        idempotency_key: format!("key-{id}"),
        definition_snapshot: json!({"sql": "SELECT 1"}),
        parameters: json!({}),
        can_retry: true,
        nodes: vec![definition("one", vec![])],
    }
}

pub fn definition(id: &str, depends_on: Vec<&str>) -> ProductJobNodeDefinition {
    ProductJobNodeDefinition {
        id: id.into(),
        name: id.into(),
        definition: json!({"sql": "SELECT 1"}),
        depends_on: depends_on.into_iter().map(String::from).collect(),
    }
}

pub fn claim_params(owner: &str) -> ClaimProductJobParams {
    ClaimProductJobParams {
        tenant_id: "a".into(),
        worker_id: owner.into(),
        lease_seconds: 30,
        source_kinds: vec!["sql_document".into()],
    }
}

pub fn lease(job: &ProductJob) -> ProductJobLease {
    ProductJobLease {
        worker_id: job.lease_owner.clone().expect("owner"),
        fence: job.lease_fence,
    }
}

pub fn lookup(id: &str) -> ProductJobLookup {
    ProductJobLookup {
        tenant_id: "a".into(),
        job_id: id.into(),
        actor_id: None,
    }
}

pub fn start(job: &ProductJob, node_id: &str) -> StartProductJobNodeParams {
    StartProductJobNodeParams {
        tenant_id: job.tenant_id.clone(),
        job_id: job.id.clone(),
        node_id: node_id.into(),
        lease: lease(job),
    }
}

pub fn node_finish(
    job: &ProductJob,
    node_id: &str,
    outcome: ProductJobOutcome,
) -> FinishProductJobNodeParams {
    FinishProductJobNodeParams {
        tenant_id: job.tenant_id.clone(),
        job_id: job.id.clone(),
        node_id: node_id.into(),
        lease: lease(job),
        outcome,
        result: matches!(outcome, ProductJobOutcome::Succeeded).then(|| json!({"rows": 1})),
        error: matches!(outcome, ProductJobOutcome::Failed).then(|| "query failed".into()),
    }
}

pub fn finish(job: &ProductJob, outcome: ProductJobOutcome) -> FinishProductJobParams {
    FinishProductJobParams {
        tenant_id: job.tenant_id.clone(),
        job_id: job.id.clone(),
        lease: lease(job),
        outcome,
        result: matches!(outcome, ProductJobOutcome::Succeeded).then(|| json!({"rows": 1})),
        error: matches!(outcome, ProductJobOutcome::Failed).then(|| "query failed".into()),
    }
}

pub async fn expire(pool: &SqlitePool, id: &str) {
    sqlx::query("UPDATE product_job SET lease_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .expect("expire test lease");
}
