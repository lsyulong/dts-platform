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


use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::middleware::auth;
use crate::models::error::AppError;

/// POST /api/v1/query/execute 请求体
#[derive(Deserialize)]
pub struct ExecuteQueryRequest {
    pub sql: String,
}

/// POST /api/v1/query/execute — 执行 SQL 查询
///
/// 将 SQL 发送到 SwanLake (DuckDB) 执行，返回列元数据 + 行数据。
/// 需要有效的 JWT token。
pub async fn execute(
    query_svc: web::Data<crate::services::query_svc::QueryService>,
    req: HttpRequest,
    body: web::Json<ExecuteQueryRequest>,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = auth::get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;

    let sql = body.sql.trim().to_string();
    if sql.is_empty() {
        return Err(AppError::BadRequest("SQL 查询不能为空".into()));
    }

    let result = query_svc.execute(&sql).await?;

    Ok(HttpResponse::Ok().json(result))
}

/// GET /api/v1/query/schemas — 列出可用 schema 和表
///
/// 返回 SwanLake 中已 ATTACH 的数据库 schema 及其表列表。
/// 需要有效的 JWT token。
pub async fn schemas(
    query_svc: web::Data<crate::services::query_svc::QueryService>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let jwt_secret = auth::get_jwt_secret(None);
    let _claims = auth::extract_claims(&req, &jwt_secret)?;

    let schemas = query_svc.list_schemas().await?;

    Ok(HttpResponse::Ok().json(schemas))
}
