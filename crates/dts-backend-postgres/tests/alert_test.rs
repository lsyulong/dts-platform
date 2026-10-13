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
use dts_backend::repo::{AlertRepo, TenantRepo};
use dts_backend_postgres::{PgAlertRepo, PgTenantRepo};
use dts_core::types::{CreateAlertParams, CreateTenantParams};

async fn setup() -> Option<(PgAlertRepo, String)> {
    let pool = setup_pool().await?;
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = PgTenantRepo::new(pool.clone());
    let slug = format!("alert-{}", uuid::Uuid::new_v4());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Alert Test".into(),
            slug,
        })
        .await
        .expect("tenant create failed");
    let repo = PgAlertRepo::new(pool);
    Some((repo, tenant_id))
}

#[tokio::test]
async fn create_and_list_alerts() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let id = uuid::Uuid::new_v4().to_string();
    let params = CreateAlertParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        task_id: None,
        name: "High Error Rate".into(),
        metric_type: "error_rate".into(),
        operator: ">".into(),
        threshold: 10.0,
        notify_channels: "email".into(),
    };
    repo.create(&params).await.expect("create failed");

    let alerts = repo.list_by_tenant(&tenant_id).await.expect("list failed");
    assert!(!alerts.is_empty());
}

#[tokio::test]
async fn toggle_alert_enabled() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateAlertParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        task_id: None,
        name: "Toggle Alert".into(),
        metric_type: "sync_delay".into(),
        operator: ">".into(),
        threshold: 300.0,
        notify_channels: "feishu".into(),
    })
    .await
    .expect("create failed");

    // Toggle off
    let result = repo.toggle(&id, &tenant_id).await.expect("toggle failed");
    assert!(!result);

    // Toggle on
    let result = repo.toggle(&id, &tenant_id).await.expect("toggle failed");
    assert!(result);
}
