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
use dts_backend::repo::{TenantRepo, UserRepo};
use dts_backend_postgres::{PgTenantRepo, PgUserRepo};
use dts_core::types::{CreateTenantParams, CreateUserParams, UpdateUserParams};

async fn setup() -> Option<(PgUserRepo, String)> {
    let pool = setup_pool().await?;
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = PgTenantRepo::new(pool.clone());
    let slug = format!("ut-{}", uuid::Uuid::new_v4());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "User Test".into(),
            slug,
        })
        .await
        .expect("tenant creation failed");

    let repo = PgUserRepo::new(pool);
    Some((repo, tenant_id))
}

#[tokio::test]
async fn create_and_find_by_username() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let user_id = uuid::Uuid::new_v4().to_string();
    let params = CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "testuser-pg".into(),
        email: "test-pg@example.com".into(),
        password_hash: "hash123".into(),
        display_name: Some("Test User".into()),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, user_id);

    let found = repo
        .find_by_tenant_and_username(&tenant_id, "testuser-pg")
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(found.username, "testuser-pg");
    assert_eq!(found.email, "test-pg@example.com");
}

#[tokio::test]
async fn find_by_id_returns_none_for_missing() {
    let Some((repo, _)) = setup().await else { return; };
    let result = repo.find_detail_by_id("nonexistent").await.expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn list_all_filters_by_tenant() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    repo.create(&CreateUserParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        username: "listuser-pg".into(),
        email: "list-pg@example.com".into(),
        password_hash: "hash".into(),
        display_name: None,
    })
    .await
    .expect("create failed");

    let users = repo
        .list_all(Some(&tenant_id), None, None)
        .await
        .expect("list failed");
    assert!(!users.is_empty());
}

#[tokio::test]
async fn update_user_info() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let user_id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "updateuser-pg".into(),
        email: "update-pg@example.com".into(),
        password_hash: "hash".into(),
        display_name: Some("Old Name".into()),
    })
    .await
    .expect("create failed");

    repo.update(
        &user_id,
        &UpdateUserParams {
            display_name: Some("New Name".into()),
            status: Some("active".into()),
        },
    )
    .await
    .expect("update failed");
}

#[tokio::test]
async fn soft_delete_user() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let user_id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "deluser-pg".into(),
        email: "del-pg@example.com".into(),
        password_hash: "hash".into(),
        display_name: None,
    })
    .await
    .expect("create failed");

    repo.soft_delete(&user_id).await.expect("soft delete failed");
}

#[tokio::test]
async fn reset_password_works() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let user_id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateUserParams {
        id: user_id.clone(),
        tenant_id: tenant_id.clone(),
        username: "pwuser-pg".into(),
        email: "pw-pg@example.com".into(),
        password_hash: "oldhash".into(),
        display_name: None,
    })
    .await
    .expect("create failed");

    repo.reset_password(&user_id, "newhash")
        .await
        .expect("reset password failed");
}
