use crate::{
    middleware::auth,
    models::error::AppError,
    services::{
        bi_share::{BiShareListQuery, BiShareService, CreateBiShareRequest, RevokeBiShareRequest},
        product_exports::ExportFormat,
    },
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

pub async fn create(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<CreateBiShareRequest>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:write").await?;
    Ok(HttpResponse::Created()
        .insert_header(("Cache-Control", "no-store"))
        .json(service.create(&actor, &path, body.into_inner()).await?))
}
pub async fn list(
    req: HttpRequest,
    query: web::Query<BiShareListQuery>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(service.list(&actor, query.into_inner()).await?))
}
pub async fn get(
    req: HttpRequest,
    path: web::Path<String>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(service.get(&actor, &path).await?))
}
pub async fn open(
    req: HttpRequest,
    path: web::Path<String>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(service.open(&actor, &path).await?))
}
pub async fn revoke(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RevokeBiShareRequest>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:write").await?;
    Ok(HttpResponse::Ok().json(service.revoke(&actor, &path, body.into_inner()).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportShare {
    pub format: ExportFormat,
}
pub async fn export(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ExportShare>,
    service: web::Data<BiShareService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "bi:read").await?;
    super::product_exports::response(service.export(&actor, &path, body.format).await?)
}
