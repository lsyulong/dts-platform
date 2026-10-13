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

use dts_backend::repo::RoleRepo;
use dts_backend::repo::TenantRepo;
use dts_backend::repo::UserRepo;
use dts_backend_rdbms::MigrationRunner;
use dts_backend_sqlite::{SqliteMigrationRunner, SqliteRoleRepo, SqliteTenantRepo, SqliteUserRepo};
use dts_core::types::{CreateRoleParams, CreateTenantParams, CreateUserParams};
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteRoleRepo, String) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed to create pool");
    let runner = SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.expect("migrations failed");

    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = SqliteTenantRepo::new(pool.clone());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Test Tenant".into(),
            slug: "test-tenant-role".into(),
        })
        .await
        .expect("tenant creation failed");

    let repo = SqliteRoleRepo::new(pool.clone());
    (pool, repo, tenant_id)
}

#[tokio::test]
async fn create_role_with_permissions() {
    let (_pool, role_repo, tenant_id) = setup().await;
    let role_id = uuid::Uuid::new_v4().to_string();

    let result = role_repo
        .create_with_permissions(&CreateRoleParams {
            id: role_id.clone(),
            tenant_id: tenant_id.clone(),
            name: "admin_role".into(),
            description: "Admin".into(),
            is_system: false,
            permissions: vec!["task:read".into(), "task:write".into()],
        })
        .await
        .expect("create failed");
    assert_eq!(result, role_id);
}

#[tokio::test]
async fn find_user_role_names_returns_assigned_roles() {
    let (pool, role_repo, tenant_id) = setup().await;

    // Create a role
    let role_id = uuid::Uuid::new_v4().to_string();
    role_repo
        .create_with_permissions(&CreateRoleParams {
            id: role_id.clone(),
            tenant_id: tenant_id.clone(),
            name: "viewer_role".into(),
            description: "Viewer".into(),
            is_system: false,
            permissions: vec!["task:read".into()],
        })
        .await
        .expect("create role failed");

    // Create a user and assign role
    let user_repo = SqliteUserRepo::new(pool.clone());
    let user_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&CreateUserParams {
            id: user_id.clone(),
            tenant_id: tenant_id.clone(),
            username: "roletest".into(),
            email: "roletest@example.com".into(),
            password_hash: "hash".into(),
            display_name: None,
        })
        .await
        .expect("create user failed");

    user_repo
        .assign_role(&user_id, &role_id)
        .await
        .expect("assign role failed");

    let role_names = role_repo
        .find_user_role_names(&user_id)
        .await
        .expect("find role names failed");
    assert!(role_names.contains(&"viewer_role".to_string()));
}

#[tokio::test]
async fn find_user_permissions_returns_correct_permissions() {
    let (pool, role_repo, tenant_id) = setup().await;

    let role_id = uuid::Uuid::new_v4().to_string();
    role_repo
        .create_with_permissions(&CreateRoleParams {
            id: role_id.clone(),
            tenant_id: tenant_id.clone(),
            name: "editor".into(),
            description: "Editor".into(),
            is_system: false,
            permissions: vec!["task:read".into(), "task:write".into()],
        })
        .await
        .expect("create role failed");

    let user_repo = SqliteUserRepo::new(pool.clone());
    let user_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&CreateUserParams {
            id: user_id.clone(),
            tenant_id: tenant_id.clone(),
            username: "permissions-user".into(),
            email: "perms@example.com".into(),
            password_hash: "hash".into(),
            display_name: None,
        })
        .await
        .expect("create user failed");

    user_repo
        .assign_role(&user_id, &role_id)
        .await
        .expect("assign role failed");

    let permissions = role_repo
        .find_user_permissions(&user_id)
        .await
        .expect("find permissions failed");
    assert!(permissions.contains(&"task:read".to_string()));
    assert!(permissions.contains(&"task:write".to_string()));
}

#[tokio::test]
async fn create_default_roles_returns_three_role_ids() {
    let (_pool, role_repo, tenant_id) = setup().await;
    let defaults = role_repo
        .create_default_roles(&tenant_id)
        .await
        .expect("create default roles failed");

    assert!(!defaults.admin_role_id.is_empty());
    assert!(!defaults.developer_role_id.is_empty());
    assert!(!defaults.viewer_role_id.is_empty());
    assert_ne!(defaults.admin_role_id, defaults.developer_role_id);
    assert_ne!(defaults.developer_role_id, defaults.viewer_role_id);
    assert_ne!(defaults.admin_role_id, defaults.viewer_role_id);
}
