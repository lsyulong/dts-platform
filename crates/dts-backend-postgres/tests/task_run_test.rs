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
use dts_backend::repo::task_run::{CreateTaskRunParams, TaskRunRepo, UpdateTaskRunParams};
use dts_backend_postgres::PgTaskRunRepo;

#[tokio::test]
async fn create_and_find_recent_runs() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTaskRunRepo::new(pool);
    let task_id = uuid::Uuid::new_v4().to_string();
    let run_id = uuid::Uuid::new_v4().to_string();

    repo.create(&CreateTaskRunParams {
        id: run_id.clone(),
        task_id: task_id.clone(),
        status: "running".into(),
    })
    .await
    .expect("create run failed");

    let runs = repo
        .find_recent(&task_id, 10)
        .await
        .expect("find recent failed");
    assert!(!runs.is_empty());
    assert_eq!(runs[0].status, "running");
}

#[tokio::test]
async fn update_task_run() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTaskRunRepo::new(pool);
    let run_id = uuid::Uuid::new_v4().to_string();
    let task_id = uuid::Uuid::new_v4().to_string();

    repo.create(&CreateTaskRunParams {
        id: run_id.clone(),
        task_id: task_id.clone(),
        status: "running".into(),
    })
    .await
    .expect("create failed");

    repo.update(&UpdateTaskRunParams {
        run_id: run_id.clone(),
        status: "completed".into(),
        rows_synced: 1000,
        bytes_synced: 5000,
        total_rows: Some(1000),
        total_bytes: Some(5000),
        end_time: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
    })
    .await
    .expect("update failed");

    let runs = repo
        .find_recent(&task_id, 10)
        .await
        .expect("find recent failed");
    assert_eq!(runs[0].status, "completed");
    assert_eq!(runs[0].rows_synced, 1000);
}

#[tokio::test]
async fn find_recent_returns_empty_for_unknown() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let repo = PgTaskRunRepo::new(pool);
    let runs = repo
        .find_recent("nonexistent-task", 10)
        .await
        .expect("query failed");
    assert!(runs.is_empty());
}
