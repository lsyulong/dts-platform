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

use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};

use crate::middleware::auth;
use crate::models::error::AppError;
use crate::services::datasource_svc::DatasourceService;
use dts_backend::repo::DatasourceRepo;

/// 创建数据源请求
#[derive(Debug, Deserialize)]
pub struct CreateDatasourceRequest {
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database_name: Option<String>,
}

/// 数据源响应
#[derive(Debug, Serialize)]
pub struct DatasourceResponse {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub database_name: Option<String>,
    pub health_status: String,
    pub last_test_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ProbeHistoryQuery {
    #[serde(default = "default_probe_limit")]
    pub limit: u32,
}

fn default_probe_limit() -> u32 {
    20
}

/// GET /api/v1/datasources
pub async fn list(
    datasource_svc: web::Data<DatasourceService>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:read").await?;

    let datasources = datasource_svc.list_by_tenant(&claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": datasources
    })))
}

/// POST /api/v1/datasources
pub async fn create(
    datasource_svc: web::Data<DatasourceService>,
    req: HttpRequest,
    body: web::Json<CreateDatasourceRequest>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:write").await?;

    let id = datasource_svc
        .create(
            &claims.tenant_id,
            &body.name,
            &body.db_type,
            &body.host,
            body.port,
            body.username.as_deref(),
            body.password.as_deref(),
            body.database_name.as_deref(),
            &claims.sub,
        )
        .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
        "db_type": body.db_type,
    })))
}

/// GET /api/v1/datasources/{id}
pub async fn get(
    ds_repo: web::Data<Arc<dyn DatasourceRepo>>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:read").await?;
    let ds_id = path.into_inner();

    let ds = ds_repo
        .find_detail_by_id(&ds_id, &claims.tenant_id)
        .await?
        .ok_or_else(|| AppError::NotFound("数据源不存在".into()))?;

    Ok(HttpResponse::Ok().json(ds))
}

/// DELETE /api/v1/datasources/{id}
pub async fn delete(
    ds_repo: web::Data<Arc<dyn DatasourceRepo>>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:write").await?;
    let ds_id = path.into_inner();

    let deleted = ds_repo.delete(&ds_id, &claims.tenant_id).await?;

    if !deleted {
        return Err(AppError::NotFound("数据源不存在".into()));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "数据源已删除"
    })))
}

/// POST /api/v1/datasources/{id}/test
pub async fn test_connection(
    datasource_svc: web::Data<DatasourceService>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:write").await?;
    let result = datasource_svc
        .probe(&claims.tenant_id, &path.into_inner(), &claims.sub)
        .await?;
    Ok(HttpResponse::Ok().json(result))
}

/// GET /api/v1/datasources/{id}/probes
pub async fn probe_history(
    datasource_svc: web::Data<DatasourceService>,
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<ProbeHistoryQuery>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:read").await?;
    let data = datasource_svc
        .probe_history(&claims.tenant_id, &path.into_inner(), query.limit)
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": data })))
}

/// GET /api/v1/datasources/{id}/tables
pub async fn tables(
    datasource_svc: web::Data<DatasourceService>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let claims = auth::require_permission(&req, "datasource:read").await?;
    let data = datasource_svc
        .discover_tables(&claims.tenant_id, &path.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "data": data })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ========== CreateDatasourceRequest Tests ==========

    #[test]
    fn create_datasource_request_with_all_fields() {
        let json = serde_json::json!({
            "name": "Production DB",
            "db_type": "MySQL",
            "host": "192.168.1.100",
            "port": 3306,
            "username": "root",
            "password": "secret123",
            "database_name": "ecommerce"
        });
        let req: CreateDatasourceRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Production DB");
        assert_eq!(req.db_type, "MySQL");
        assert_eq!(req.host, "192.168.1.100");
        assert_eq!(req.port, 3306);
        assert_eq!(req.username, Some("root".into()));
        assert_eq!(req.password, Some("secret123".into()));
        assert_eq!(req.database_name, Some("ecommerce".into()));
    }

    #[test]
    fn create_datasource_request_minimal_fields() {
        let json = serde_json::json!({
            "name": "Minimal DB",
            "db_type": "SQLite",
            "host": "localhost",
            "port": 0
        });
        let req: CreateDatasourceRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.name, "Minimal DB");
        assert_eq!(req.db_type, "SQLite");
        assert_eq!(req.host, "localhost");
        assert_eq!(req.port, 0);
        assert_eq!(req.username, None);
        assert_eq!(req.password, None);
        assert_eq!(req.database_name, None);
    }

    #[test]
    fn create_datasource_request_various_db_types() {
        for db_type in &["MySQL", "PostgreSQL", "SQLite", "SQL Server", "Oracle"] {
            let json = serde_json::json!({
                "name": format!("{} DB", db_type),
                "db_type": db_type,
                "host": "localhost",
                "port": 3306
            });
            let req: CreateDatasourceRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.db_type, *db_type);
        }
    }

    #[test]
    fn datasource_response_serializes_correctly() {
        let ds = DatasourceResponse {
            id: "ds-1".into(),
            name: "My DB".into(),
            db_type: "MySQL".into(),
            host: "localhost".into(),
            port: 3306,
            username: Some("admin".into()),
            database_name: Some("mydb".into()),
            health_status: "healthy".into(),
            last_test_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&ds).unwrap();
        assert_eq!(json["id"], "ds-1");
        assert_eq!(json["db_type"], "MySQL");
        assert_eq!(json["health_status"], "healthy");
        assert_eq!(json["username"], "admin");
    }
}
