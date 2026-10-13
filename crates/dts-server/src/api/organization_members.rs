use crate::{middleware::auth, models::error::AppError, services::organization_members::*};
use actix_web::{HttpRequest, HttpResponse, web};

pub async fn organization(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.organization(&actor, &path).await?))
}
pub async fn membership(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.membership(&actor, &path).await?))
}
pub async fn members(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<OrganizationMembersQuery>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.members(&actor, &path, query.into_inner()).await?))
}
pub async fn transfer(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<TransferOrganizationMemberRequest>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(svc.transfer(&actor, &path, body.into_inner()).await?))
}
pub async fn remove(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RemoveOrganizationMemberRequest>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(svc.remove(&actor, &path, body.into_inner()).await?))
}
pub async fn manager(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetOrganizationManagerRequest>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:write").await?;
    auth::require_permission(&req, "user:manage").await?;
    Ok(HttpResponse::Ok().json(svc.manager(&actor, &path, body.into_inner()).await?))
}
pub async fn history(
    req: HttpRequest,
    query: web::Query<OrganizationHistoryQuery>,
    svc: web::Data<OrganizationMembersService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "system:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.history(&actor, query.into_inner()).await?))
}
