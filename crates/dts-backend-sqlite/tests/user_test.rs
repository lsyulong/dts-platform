// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use dts_backend::repo::TenantRepo;
use dts_backend::repo::UserRepo;
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteTenantRepo, SqliteUserRepo};
use dts_core::types::{CreateTenantParams, CreateUserParams};
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteUserRepo, String) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    // Create a tenant first for FK constraint
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = SqliteTenantRepo::new(pool.clone());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Test Tenant".into(),
            slug: "test-tenant".into(),
        })
        .await
        .expect("tenant creation failed");

    let repo = SqliteUserRepo::new(pool.clone());
    (pool, repo, tenant_id)
}

#[tokio::test]
async fn create_and_find_by_username() {
    let (_pool, repo, tenant_id) = setup().await;
    let user_id = uuid::Uuid::new_v4().to_string();
    let params = CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "testuser".into(),
        email: "test@example.com".into(),
        password_hash: "hash123".into(),
        display_name: Some("Test User".into()),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, user_id);

    let found = repo
        .find_by_tenant_and_username(&tenant_id, "testuser")
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(found.username, "testuser");
    assert_eq!(found.email, "test@example.com");
}

#[tokio::test]
async fn find_by_id_returns_none_for_missing() {
    let (_pool, repo, _tenant_id) = setup().await;
    let result = repo
        .find_detail_by_id("nonexistent")
        .await
        .expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn find_detail_by_id_returns_user_with_tenant() {
    let (_pool, repo, tenant_id) = setup().await;
    let user_id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "detailuser".into(),
        email: "detail@example.com".into(),
        password_hash: "hash456".into(),
        display_name: None,
    })
    .await
    .expect("create failed");

    let detail = repo
        .find_detail_by_id(&user_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.username, "detailuser");
    assert_eq!(detail.tenant_name, "Test Tenant");
    assert!(detail.roles.is_empty());
    assert!(detail.permissions.is_empty());
}

#[tokio::test]
async fn assign_role_adds_user_role_mapping() {
    let (pool, user_repo, tenant_id) = setup().await;

    // Create a role using raw SQL for test setup
    let role_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO role (id, tenant_id, name, description, is_system) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&role_id)
    .bind(&tenant_id)
    .bind("test_role")
    .bind("Test role")
    .bind(true)
    .execute(&pool)
    .await
    .expect("role creation failed");

    let user_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&CreateUserParams {
            id: user_id.clone(),
            tenant_id: tenant_id.clone(),
            username: "roleuser".into(),
            email: "role@example.com".into(),
            password_hash: "hash789".into(),
            display_name: None,
        })
        .await
        .expect("create failed");

    user_repo
        .assign_role(&user_id, &role_id)
        .await
        .expect("assign_role failed");

    // Verify via UserDetail
    let detail = user_repo
        .find_detail_by_id(&user_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert!(detail.roles.contains(&"test_role".to_string()));
}
