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
use crate::{
    models::auth::AuthIdentity,
    services::data_access::{AccessOutcome, QueryAuthorizationService},
};

/// POST /api/v1/query/execute 请求体
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteQueryRequest {
    pub sql: String,
}

/// Each configured analytical endpoint belongs to exactly one tenant.
#[derive(Clone)]
pub struct QueryAccess {
    pub tenant_id: Option<String>,
}

async fn authorize_query(req: &HttpRequest) -> Result<AuthIdentity, AppError> {
    let claims = auth::require_permission(req, "task:execute").await?;
    let access = req
        .app_data::<web::Data<QueryAccess>>()
        .ok_or_else(|| AppError::Internal("查询租户绑定未配置".into()))?;
    match access.tenant_id.as_deref() {
        Some(id) if id == claims.tenant_id => Ok(claims),
        Some(_) => Err(AppError::Forbidden("当前租户未配置查询引擎".into())),
        None => Err(AppError::BadRequest("SwanLake 查询服务未启用".into())),
    }
}

fn authorization(req: &HttpRequest) -> Result<&web::Data<QueryAuthorizationService>, AppError> {
    req.app_data::<web::Data<QueryAuthorizationService>>()
        .ok_or_else(|| AppError::Internal("查询数据授权服务未配置".into()))
}

/// POST /api/v1/query/execute — 执行 SQL 查询
///
/// 将 SQL 发送到 SwanLake (DuckDB) 执行，返回列元数据 + 行数据。
pub async fn execute(
    query_svc: web::Data<crate::services::query_svc::QueryService>,
    req: HttpRequest,
    body: web::Json<ExecuteQueryRequest>,
) -> Result<HttpResponse, AppError> {
    let actor = authorize_query(&req).await?;

    let sql = body.sql.trim().to_string();
    if sql.is_empty() {
        return Err(AppError::BadRequest("SQL 查询不能为空".into()));
    }

    let guard = authorization(&req)?;
    let authorized = guard.authorize(&actor, &sql).await?;
    let masking = req
        .app_data::<web::Data<crate::services::data_masking::MaskingService>>()
        .ok_or_else(|| AppError::Internal("查询脱敏服务未配置".into()))?;
    let plan = masking.prepare_output(&actor, &authorized).await?;
    guard.reauthorize(&actor, &authorized).await?;
    masking.reauthorize_plan(&actor, &plan).await?;
    let result = match query_svc.execute(authorized.sql()).await {
        Ok(result) => result,
        Err(error) => {
            guard
                .record_access(&actor, &authorized, AccessOutcome::Failed)
                .await?;
            return Err(error);
        },
    };
    if let Err(error) = guard.reauthorize(&actor, &authorized).await {
        guard
            .record_access(&actor, &authorized, AccessOutcome::Denied)
            .await?;
        return Err(error);
    }
    let result = match masking.apply_output(&plan, result) {
        Ok(result) => result,
        Err(error) => {
            guard
                .record_access(&actor, &authorized, AccessOutcome::Denied)
                .await?;
            return Err(error);
        },
    };
    if let Err(error) = masking.reauthorize_plan(&actor, &plan).await {
        guard
            .record_access(&actor, &authorized, AccessOutcome::Denied)
            .await?;
        return Err(error);
    }
    if let Err(error) = guard.reauthorize(&actor, &authorized).await {
        guard
            .record_access(&actor, &authorized, AccessOutcome::Denied)
            .await?;
        return Err(error);
    }
    guard
        .record_access(&actor, &authorized, AccessOutcome::Succeeded)
        .await?;

    Ok(HttpResponse::Ok().json(result))
}

/// GET /api/v1/query/schemas — 列出可用 schema 和表
///
/// 返回 SwanLake 中已 ATTACH 的数据库 schema 及其表列表。
pub async fn schemas(
    query_svc: web::Data<crate::services::query_svc::QueryService>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let actor = authorize_query(&req).await?;
    let guard = authorization(&req)?;
    let schemas = if guard.has_all_data(&actor).await? {
        let schemas = query_svc.list_schemas().await?;
        if !guard.has_all_data(&actor).await? {
            return Err(AppError::Forbidden("查询过程中全量目录权限已撤回".into()));
        }
        schemas
    } else {
        guard.visible_schemas(&actor).await?
    };

    Ok(HttpResponse::Ok().json(schemas))
}
