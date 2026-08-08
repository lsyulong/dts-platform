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
use dts_backend::{BackendError, repo::DatasourceRepo, types::*};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::PgPool;
use sqlx::Row;

pub struct PgDatasourceRepo {
    pool: PgPool,
}

impl PgDatasourceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::postgres::PgRow) -> DatasourceInfo {
        DatasourceInfo {
            id: r.get(0),
            name: r.get(1),
            db_type: r.get(2),
            host: r.get(3),
            port: r.get(4),
            username: r.get(5),
            database_name: r.get(6),
            health_status: r.get(7),
            created_at: r.get(8),
        }
    }
}

#[async_trait]
impl DatasourceRepo for PgDatasourceRepo {
    async fn create(&self, params: &CreateDatasourceParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, username,
             encrypted_password, database_name, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.db_type)
        .bind(&params.host)
        .bind(params.port)
        .bind(&params.username)
        .bind(&params.encrypted_password)
        .bind(&params.database_name)
        .bind(&params.created_by)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, name, db_type, host, port, username, database_name,
             health_status, created_at
             FROM datasource WHERE tenant_id = $1
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.iter().map(Self::row_to_info).collect())
    }

    async fn find_detail_by_id(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<DatasourceDetail>, BackendError> {
        let row = sqlx::query(
            "SELECT id, name, db_type, host, port, username, encrypted_password,
             database_name, extra_params, health_status, last_test_at, created_at, updated_at
             FROM datasource WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => {
                let encrypted: Option<String> = r.get(6);
                let last_test_at: Option<chrono::DateTime<chrono::Utc>> = r.get(10);
                let created_at: chrono::DateTime<chrono::Utc> = r.get(11);
                let updated_at: chrono::DateTime<chrono::Utc> = r.get(12);
                Ok(Some(DatasourceDetail {
                    id: r.get(0),
                    name: r.get(1),
                    db_type: r.get(2),
                    host: r.get(3),
                    port: r.get(4),
                    username: r.get(5),
                    password_masked: encrypted.map(|_| "******".to_string()),
                    database_name: r.get(7),
                    extra_params: r.get(8),
                    health_status: r.get(9),
                    last_test_at: last_test_at.map(|dt| dt.to_rfc3339()),
                    created_at: created_at.to_rfc3339(),
                    updated_at: updated_at.to_rfc3339(),
                }))
            },
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        let result = sqlx::query("DELETE FROM datasource WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)
    }

    async fn count_healthy_by_tenant(&self, tenant_id: &str) -> Result<i64, BackendError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM datasource WHERE tenant_id = $1 AND health_status = 'healthy'",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn exists(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM datasource WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        Ok(count > 0)
    }

    async fn get_connection_info(
        &self,
        id: &str,
        tenant_id: &str,
    ) -> Result<Option<dts_backend::repo::RawConnectionInfo>, BackendError> {
        let row = sqlx::query(
            "SELECT db_type, host, port, username, encrypted_password, database_name
             FROM datasource WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match row {
            Some(r) => Ok(Some(dts_backend::repo::RawConnectionInfo {
                db_type: r.get(0),
                host: r.get(1),
                port: r.get(2),
                username: r.get(3),
                password: r.get(4),
                database_name: r.get(5),
            })),
            None => Ok(None),
        }
    }
}
