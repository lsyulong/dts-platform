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
use dts_backend::repo::{MonitoringRepo, TaskRepo, TenantRepo};
use dts_backend_postgres::{PgMonitoringRepo, PgTaskRepo, PgTenantRepo};
use dts_core::types::{CreateTaskParams, CreateTenantParams};

async fn setup_with_data() -> Option<(PgMonitoringRepo, String)> {
    let pool = setup_pool().await?;
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = PgTenantRepo::new(pool.clone());
    let slug = format!("mon-{}", uuid::Uuid::new_v4());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "Monitoring Test".into(),
            slug,
        })
        .await
        .expect("tenant create failed");

    let task_repo = PgTaskRepo::new(pool.clone());
    for i in 0..3 {
        task_repo
            .create(&CreateTaskParams {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: tenant_id.clone(),
                name: format!("MonTask-{i}"),
                description: None,
                source_id: "s".into(),
                target_id: "t".into(),
                cron_expr: None,
                config_json: "{}".into(),
                created_by: "admin".into(),
            })
            .await
            .expect("task create failed");
    }

    let repo = PgMonitoringRepo::new(pool);
    Some((repo, tenant_id))
}

#[tokio::test]
async fn get_overview_returns_stats() {
    let Some((repo, tenant_id)) = setup_with_data().await else { return; };
    let overview = repo
        .get_overview(&tenant_id)
        .await
        .expect("overview failed");
    assert!(overview.total_tasks >= 3);
    assert!(overview.total_datasources >= 0);
}

#[tokio::test]
async fn get_overview_for_empty_tenant() {
    let Some(pool) = setup_pool().await else { return; };
    let repo = PgMonitoringRepo::new(pool);
    let overview = repo
        .get_overview("nonexistent-tenant")
        .await
        .expect("overview failed");
    assert_eq!(overview.total_tasks, 0);
    assert_eq!(overview.success_rate, 0.0);
}
