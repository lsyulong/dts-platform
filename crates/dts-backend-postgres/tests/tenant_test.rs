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
use dts_backend::repo::TenantRepo;
use dts_backend_postgres::PgTenantRepo;
use dts_core::types::CreateTenantParams;

#[tokio::test]
async fn create_and_find_tenant() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTenantRepo::new(pool);
    let id = uuid::Uuid::new_v4().to_string();
    let params = CreateTenantParams {
        id: id.clone(),
        name: "Test Tenant".into(),
        slug: "test-create-pg".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let found = repo
        .find_by_slug("test-create-pg")
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(found.name, "Test Tenant");
    assert_eq!(found.status, "active");
}

#[tokio::test]
async fn find_by_id_returns_none_for_missing() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTenantRepo::new(pool);
    let result = repo.find_by_id("nonexistent").await.expect("query failed");
    assert!(result.is_none());
}

#[tokio::test]
async fn create_duplicate_slug_returns_error() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTenantRepo::new(pool);
    let slug = "duplicate-slug";
    repo.create(&CreateTenantParams {
        id: uuid::Uuid::new_v4().to_string(),
        name: "First".into(),
        slug: slug.into(),
    })
    .await
    .expect("first create failed");

    let result = repo
        .create(&CreateTenantParams {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Second".into(),
            slug: slug.into(),
        })
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_all_returns_tenants() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTenantRepo::new(pool);
    let tenants = repo.list_all().await.expect("list failed");
    // May or may not have tenants from other tests — just verify it doesn't crash
    let _ = tenants.len();
}

#[tokio::test]
async fn count_returns_non_negative() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTenantRepo::new(pool);
    let count = repo.count().await.expect("count failed");
    assert!(count >= 0);
}
