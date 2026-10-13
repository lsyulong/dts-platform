use crate::{
    middleware::auth,
    models::error::AppError,
    services::navigation::{AssignNavigationRequest, NavigationService, RevokeNavigationRequest},
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

pub async fn current(
    req: HttpRequest,
    svc: web::Data<NavigationService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.current(&actor).await?))
}
pub async fn assign(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<AssignNavigationRequest>,
    svc: web::Data<NavigationService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(svc.assign(&actor, &path, body.into_inner()).await?))
}
pub async fn revoke(
    req: HttpRequest,
    body: web::Json<RevokeNavigationRequest>,
    svc: web::Data<NavigationService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(svc.revoke(&actor, body.into_inner()).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}
pub async fn history(
    req: HttpRequest,
    query: web::Query<HistoryQuery>,
    svc: web::Data<NavigationService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(
        svc.history(&actor, query.limit.unwrap_or(20), query.offset.unwrap_or(0))
            .await?,
    ))
}
