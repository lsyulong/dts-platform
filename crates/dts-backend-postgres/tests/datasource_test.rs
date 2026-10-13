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
use dts_backend::repo::{DatasourceRepo, TenantRepo};
use dts_backend_postgres::{PgDatasourceRepo, PgTenantRepo};
use dts_core::types::{CreateDatasourceParams, CreateTenantParams};

async fn setup() -> Option<(PgDatasourceRepo, String)> {
    let pool = setup_pool().await?;
    let tenant_id = uuid::Uuid::new_v4().to_string();
    let tenant_repo = PgTenantRepo::new(pool.clone());
    let slug = format!("ds-{}", uuid::Uuid::new_v4());
    tenant_repo
        .create(&CreateTenantParams {
            id: tenant_id.clone(),
            name: "DS Test".into(),
            slug,
        })
        .await
        .expect("tenant create failed");
    let repo = PgDatasourceRepo::new(pool);
    Some((repo, tenant_id))
}

#[tokio::test]
async fn create_and_list_datasource() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let id = uuid::Uuid::new_v4().to_string();
    let params = CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Test DS".into(),
        db_type: "postgresql".into(),
        host: "localhost".into(),
        port: 5432,
        username: Some("test".into()),
        encrypted_password: Some("secret".into()),
        database_name: Some("testdb".into()),
        created_by: "admin".into(),
    };
    let result = repo.create(&params).await.expect("create failed");
    assert_eq!(result, id);

    let list = repo.list_by_tenant(&tenant_id).await.expect("list failed");
    assert!(!list.is_empty());
}

#[tokio::test]
async fn find_detail_by_id_returns_correct_fields() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "Detail DS".into(),
        db_type: "mysql".into(),
        host: "db.example.com".into(),
        port: 3306,
        username: Some("root".into()),
        encrypted_password: Some("pass".into()),
        database_name: Some("mydb".into()),
        created_by: "admin".into(),
    })
    .await
    .expect("create failed");

    let detail = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find failed")
        .expect("not found");
    assert_eq!(detail.name, "Detail DS");
    assert_eq!(detail.db_type, "mysql");
    assert_eq!(detail.host, "db.example.com");
}

#[tokio::test]
async fn delete_datasource() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let id = uuid::Uuid::new_v4().to_string();
    repo.create(&CreateDatasourceParams {
        id: id.clone(),
        tenant_id: tenant_id.clone(),
        name: "To Delete".into(),
        db_type: "postgresql".into(),
        host: "localhost".into(),
        port: 5432,
        username: Some("test".into()),
        encrypted_password: Some("x".into()),
        database_name: Some("x".into()),
        created_by: "admin".into(),
    })
    .await
    .expect("create failed");

    let deleted = repo.delete(&id, &tenant_id).await.expect("delete failed");
    assert!(deleted);

    let after = repo
        .find_detail_by_id(&id, &tenant_id)
        .await
        .expect("find failed");
    assert!(after.is_none());
}

#[tokio::test]
async fn count_by_tenant_returns_non_negative() {
    let Some((repo, tenant_id)) = setup().await else {
        return;
    };
    let count = repo
        .count_by_tenant(&tenant_id)
        .await
        .expect("count failed");
    assert!(count >= 0);
}
