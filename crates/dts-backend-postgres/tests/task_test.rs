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
use dts_backend::repo::{TaskRepo, TenantRepo};
use dts_backend_postgres::{PgTaskRepo, PgTenantRepo};
use dts_core::types::{CreateTaskParams, CreateTenantParams};

async fn setup() -> Option<(PgTaskRepo, String)> {
    let pool = setup_pool().await?;
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = PgTenantRepo::new(pool.clone());
    let slug = format!("task-{}", uuid::Uuid::new_v4());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Task Test".into(),
            slug,
        })
        .await
        .expect("tenant create failed");
    let repo = PgTaskRepo::new(pool);
    Some((repo, tenant_id))
}

#[tokio::test]
async fn create_and_find_task() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let id = uuid::Uuid::new_v4().to_string();
    let params = CreateTaskParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Sync Task".into(),
        description: Some("Test sync task".into()),
        source_id: "src-1".into(),
        target_id: "tgt-1".into(),
        cron_expr: Some("0 */6 * * *".into()),
        config_json: "{}".into(),
        created_by: "admin".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let detail = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.name, "Sync Task");
    assert_eq!(detail.status, "draft");
}

#[tokio::test]
async fn list_by_tenant_filters_correctly() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    repo.create(&CreateTaskParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "List Task".into(),
        description: None,
        source_id: "s".into(),
        target_id: "t".into(),
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "admin".into(),
    })
    .await
    .expect("create failed");

    let tasks = repo
        .list_by_tenant(&tenant_id)
        .await
        .expect("list failed");
    assert_eq!(tasks.len(), 1);
}

#[tokio::test]
async fn update_status_works() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    let id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateTaskParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Status Task".into(),
        description: None,
        source_id: "s".into(),
        target_id: "t".into(),
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "admin".into(),
    })
    .await
    .expect("create failed");

    let updated = repo
        .update_status(&id, &tenant_id, "running")
        .await
        .expect("update failed");
    assert!(updated);

    let detail = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.status, "running");
}

#[tokio::test]
async fn count_by_status_returns_correct_count() {
    let Some((repo, tenant_id)) = setup().await else { return; };
    repo.create(&CreateTaskParams {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        name: "Count Task".into(),
        description: None,
        source_id: "s".into(),
        target_id: "t".into(),
        cron_expr: None,
        config_json: "{}".into(),
        created_by: "admin".into(),
    })
    .await
    .expect("create failed");

    let draft_count = repo
        .count_by_status(&tenant_id, "draft")
        .await
        .expect("count failed");
    assert!(draft_count >= 1);
}
