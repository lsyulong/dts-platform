use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;
use dts_backend::repo::DatasourceRepo;
use dts_backend::types::CreateDatasourceParams;

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
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// GET /api/v1/datasources
pub async fn list(
    ds_repo: web::Data<Arc<dyn DatasourceRepo>>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let datasources = ds_repo.list_by_tenant(&claims.tenant_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": datasources
    })))
}

/// POST /api/v1/datasources
pub async fn create(
    ds_repo: web::Data<Arc<dyn DatasourceRepo>>,
    req: HttpRequest,
    body: web::Json<CreateDatasourceRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let id = Uuid::new_v4().to_string();
    let encrypted_pw = body.password.as_ref().map(|p| format!("enc:{p}"));

    ds_repo
        .create(&CreateDatasourceParams {
            id: id.clone(),
            tenant_id: claims.tenant_id,
            name: body.name.clone(),
            db_type: body.db_type.clone(),
            host: body.host.clone(),
            port: body.port,
            username: body.username.clone(),
            encrypted_password: encrypted_pw,
            database_name: body.database_name.clone(),
            created_by: claims.sub,
        })
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
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
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
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let ds_id = path.into_inner();

    let deleted = ds_repo.delete(&ds_id, &claims.tenant_id).await?;

    if !deleted {
        return Err(AppError::NotFound("数据源不存在".into()));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "数据源已删除"
    })))
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
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&ds).unwrap();
        assert_eq!(json["id"], "ds-1");
        assert_eq!(json["db_type"], "MySQL");
        assert_eq!(json["health_status"], "healthy");
        assert_eq!(json["username"], "admin");
    }
}
