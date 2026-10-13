// Copyright 2026 DTS Platform Contributors
// SPDX-License-Identifier: Apache-2.0

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth,
    models::error::AppError,
    services::product_jobs::{JobListQuery, ProductJobService, RetryRequest, RunRequest},
};

pub async fn enqueue(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<RunRequest>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    let (kind, id) = path.into_inner();
    let result = service
        .enqueue(&actor, &kind, &id, body.into_inner())
        .await?;
    Ok(HttpResponse::Accepted().json(service.submission_view(&actor, result).await?))
}

pub async fn list(
    req: HttpRequest,
    query: web::Query<JobListQuery>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(service.list(&actor, query.into_inner()).await?))
}

pub async fn get(
    req: HttpRequest,
    path: web::Path<String>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(service.get(&actor, &path).await?))
}

pub async fn cancel(
    req: HttpRequest,
    path: web::Path<String>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(service.cancel(&actor, &path).await?))
}

pub async fn retry(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RetryRequest>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    let result = service.retry(&actor, &path, body.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(service.submission_view(&actor, result).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionVersion {
    pub expected_version: i64,
}

pub async fn validate(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<DefinitionVersion>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    let (kind, id) = path.into_inner();
    Ok(HttpResponse::Ok().json(
        service
            .validate(&actor, &kind, &id, body.expected_version)
            .await?,
    ))
}

pub async fn publish(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<DefinitionVersion>,
    service: web::Data<ProductJobService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    let (kind, id) = path.into_inner();
    Ok(HttpResponse::Ok().json(
        service
            .publish(&actor, &kind, &id, body.expected_version)
            .await?,
    ))
}
