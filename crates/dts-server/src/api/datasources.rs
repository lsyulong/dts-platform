use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool}; // Already correct
use uuid::Uuid;

use crate::middleware::auth;
use crate::middleware::auth::get_jwt_secret;
use crate::models::error::AppError;

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
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DatasourceResponse {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub database_name: Option<String>,
    pub health_status: String,
    pub created_at: String,
}

/// GET /api/v1/datasources
pub async fn list(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let datasources: Vec<DatasourceResponse> = sqlx::query_as::<_, DatasourceResponse>(
        "SELECT id, name, db_type, host, port, username, database_name,
         health_status, created_at
         FROM datasource WHERE tenant_id = ?
         ORDER BY created_at DESC"
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool.get_ref())
    .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": datasources
    })))
}

/// POST /api/v1/datasources
pub async fn create(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    body: web::Json<CreateDatasourceRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;

    let id = Uuid::new_v4().to_string();

    let encrypted_pw = body
        .password
        .as_ref()
        .map(|p| format!("enc:{}", p));

    sqlx::query(
        "INSERT INTO datasource (id, tenant_id, name, db_type, host, port, username,
         encrypted_password, database_name, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&claims.tenant_id)
    .bind(&body.name)
    .bind(&body.db_type)
    .bind(&body.host)
    .bind(body.port)
    .bind(&body.username)
    .bind(&encrypted_pw)
    .bind(&body.database_name)
    .bind(&claims.sub)
    .execute(pool.get_ref())
    .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
        "db_type": body.db_type,
    })))
}

/// GET /api/v1/datasources/{id}
pub async fn get(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let ds_id = path.into_inner();

    let ds = sqlx::query(
        "SELECT id, name, db_type, host, port, username, encrypted_password,
         database_name, extra_params, health_status, last_test_at, created_at, updated_at
         FROM datasource WHERE id = ? AND tenant_id = ?"
    )
    .bind(&ds_id)
    .bind(&claims.tenant_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("数据源不存在".into()))?;

    let encrypted: Option<String> = ds.get(6);
    let password_masked = encrypted.map(|_| "******".to_string());

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": ds.get::<String, _>(0),
        "name": ds.get::<String, _>(1),
        "db_type": ds.get::<String, _>(2),
        "host": ds.get::<String, _>(3),
        "port": ds.get::<i64, _>(4),
        "username": ds.get::<Option<String>, _>(5),
        "password": password_masked,
        "database_name": ds.get::<Option<String>, _>(7),
        "extra_params": ds.get::<Option<String>, _>(8),
        "health_status": ds.get::<String, _>(9),
        "last_test_at": ds.get::<Option<String>, _>(10),
        "created_at": ds.get::<String, _>(11),
        "updated_at": ds.get::<String, _>(12),
    })))
}

/// DELETE /api/v1/datasources/{id}
pub async fn delete(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = get_jwt_secret(None);
    let claims = auth::extract_claims(&req, &jwt_secret)?;
    let ds_id = path.into_inner();

    let result = sqlx::query(
        "DELETE FROM datasource WHERE id = ? AND tenant_id = ?"
    )
    .bind(&ds_id)
    .bind(&claims.tenant_id)
    .execute(pool.get_ref())
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("数据源不存在".into()));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "数据源已删除"
    })))
}

// get_jwt_secret 已集中到 crate::middleware::auth

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
    fn create_datasource_request_missing_name_fails() {
        let json = serde_json::json!({
            "db_type": "MySQL",
            "host": "localhost",
            "port": 3306
        });
        let result = serde_json::from_value::<CreateDatasourceRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_datasource_request_missing_db_type_fails() {
        let json = serde_json::json!({
            "name": "DB",
            "host": "localhost",
            "port": 3306
        });
        let result = serde_json::from_value::<CreateDatasourceRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_datasource_request_missing_host_fails() {
        let json = serde_json::json!({
            "name": "DB",
            "db_type": "MySQL",
            "port": 3306
        });
        let result = serde_json::from_value::<CreateDatasourceRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_datasource_request_missing_port_fails() {
        let json = serde_json::json!({
            "name": "DB",
            "db_type": "MySQL",
            "host": "localhost"
        });
        let result = serde_json::from_value::<CreateDatasourceRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn create_datasource_request_with_null_optionals() {
        let json = serde_json::json!({
            "name": "DB",
            "db_type": "PostgreSQL",
            "host": "pg.example.com",
            "port": 5432,
            "username": null,
            "password": null,
            "database_name": null
        });
        let req: CreateDatasourceRequest = serde_json::from_value(json).unwrap();
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

    // ========== DatasourceResponse Tests ==========

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
            created_at: "2024-01-01".into(),
        };
        let json = serde_json::to_value(&ds).unwrap();
        assert_eq!(json["id"], "ds-1");
        assert_eq!(json["db_type"], "MySQL");
        assert_eq!(json["health_status"], "healthy");
        assert_eq!(json["username"], "admin");
    }
}
