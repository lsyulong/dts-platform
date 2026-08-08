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


use async_trait::async_trait;
use dts_backend::{BackendError, repo::TenantRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgTenantRepo {
    pool: PgPool,
}

impl PgTenantRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::postgres::PgRow) -> TenantInfo {
        TenantInfo {
            id: r.get(0),
            name: r.get(1),
            slug: r.get(2),
            status: r.get(3),
            created_at: r.get(4),
        }
    }
}

#[async_trait]
impl TenantRepo for PgTenantRepo {
    async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError> {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES ($1, $2, $3)")
            .bind(&params.id)
            .bind(&params.name)
            .bind(&params.slug)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row =
            sqlx::query("SELECT id, name, slug, status, created_at FROM tenant WHERE slug = $1")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row =
            sqlx::query("SELECT id, name, slug, status, created_at FROM tenant WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, name, slug, status, created_at FROM tenant ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.iter().map(Self::row_to_info).collect())
    }

    async fn count(&self) -> Result<i64, BackendError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)
    }

    async fn update(
        &self,
        tenant_id: &str,
        params: &UpdateTenantParams,
    ) -> Result<(), BackendError> {
        if let Some(ref name) = params.name {
            sqlx::query("UPDATE tenant SET name = $1 WHERE id = $2")
                .bind(name)
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        if let Some(ref status) = params.status {
            sqlx::query("UPDATE tenant SET status = $1 WHERE id = $2")
                .bind(status)
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        Ok(())
    }
}
