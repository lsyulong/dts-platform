use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool}; // Already correct
use uuid::Uuid;

use crate::middleware::auth;
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
    let jwt_secret = get_jwt_secret(&req);
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
    let jwt_secret = get_jwt_secret(&req);
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
    let jwt_secret = get_jwt_secret(&req);
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
    let jwt_secret = get_jwt_secret(&req);
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

fn get_jwt_secret(_req: &HttpRequest) -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string())
}
