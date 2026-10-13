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

mod common;

use common::setup_pool;
use dts_backend::repo::RoleRepo;
use dts_backend_postgres::PgRoleRepo;

#[tokio::test]
async fn create_default_roles_returns_ids() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgRoleRepo::new(pool);
    let tenant_id = uuid::Uuid::new_v4().to_string();

    let default_roles = repo
        .create_default_roles(&tenant_id)
        .await
        .expect("create default roles failed");
    assert!(!default_roles.admin_role_id.is_empty());
    assert!(!default_roles.developer_role_id.is_empty());
    assert!(!default_roles.viewer_role_id.is_empty());
}

#[tokio::test]
async fn list_all_returns_roles() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgRoleRepo::new(pool);
    let tenant_id = uuid::Uuid::new_v4().to_string();
    repo.create_default_roles(&tenant_id)
        .await
        .expect("create default roles failed");

    let roles = repo.list_all(Some(&tenant_id)).await.expect("list failed");
    assert!(!roles.is_empty());
}

#[tokio::test]
async fn find_user_role_names_returns_empty_for_new_user() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgRoleRepo::new(pool);
    let names = repo
        .find_user_role_names("nonexistent-user")
        .await
        .expect("query failed");
    assert!(names.is_empty());
}

#[tokio::test]
async fn find_all_permissions_returns_list() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgRoleRepo::new(pool);
    let perms = repo.find_all_permissions().await.expect("query failed");
    // System has pre-defined permissions
    assert!(!perms.is_empty());
}
