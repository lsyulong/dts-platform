use crate::{middleware::auth, models::error::AppError, services::development_revisions::*};
use actix_web::{HttpRequest, HttpResponse, web};
fn service(req: &HttpRequest) -> Result<&web::Data<DevelopmentRevisionsService>, AppError> {
    req.app_data::<web::Data<DevelopmentRevisionsService>>()
        .ok_or_else(|| AppError::Internal("开发修订服务未配置".into()))
}
pub async fn history(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    q: web::Query<DevelopmentRevisionQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:read").await?;
    let path = path.into_inner();
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .history(&actor, &path.0, &path.1, q.into_inner())
                .await?,
        ))
}
pub async fn detail(
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:read").await?;
    let path = path.into_inner();
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .detail(&actor, &path.0, &path.1, &path.2)
                .await?,
        ))
}
pub async fn diff(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    q: web::Query<DevelopmentRevisionDiffQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:read").await?;
    let path = path.into_inner();
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .diff(&actor, &path.0, &path.1, q.into_inner())
                .await?,
        ))
}
pub async fn save(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<SaveDevelopmentRevisionRequest>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:write").await?;
    let path = path.into_inner();
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .save(&actor, &path.0, &path.1, body.into_inner())
                .await?,
        ))
}
pub async fn restore(
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
    body: web::Json<RestoreDevelopmentRevisionRequest>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:write").await?;
    let path = path.into_inner();
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .restore(&actor, &path.0, &path.1, &path.2, body.into_inner())
                .await?,
        ))
}
