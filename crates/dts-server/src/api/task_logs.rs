// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth,
    models::error::AppError,
    services::task_log_svc::{RunLogScope, TaskLogService},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsQuery {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 {
    200
}

pub async fn list(
    req: HttpRequest,
    service: web::Data<TaskLogService>,
    path: web::Path<(String, String)>,
    query: web::Query<LogsQuery>,
) -> Result<HttpResponse, AppError> {
    let identity = auth::require_permission(&req, "task:read").await?;
    if query.offset < 0 || !(1..=200).contains(&query.limit) {
        return Err(AppError::BadRequest(
            "offset 不得为负，limit 必须在 1 到 200 之间".into(),
        ));
    }
    let (task_id, run_id) = path.into_inner();
    let page = service
        .list(
            RunLogScope {
                tenant_id: &identity.tenant_id,
                task_id: &task_id,
                run_id: &run_id,
            },
            query.offset,
            query.limit,
        )
        .await?;
    Ok(HttpResponse::Ok().json(page))
}
