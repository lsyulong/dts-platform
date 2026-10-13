use crate::{
    middleware::auth,
    models::error::AppError,
    services::quality_alerts::{QualityAckRequest, QualityAlertsService, QualityEvaluateRequest},
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

fn limit() -> u32 {
    20
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertQuery {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    status: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeQuery {
    #[serde(default = "limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    unread_only: bool,
}
pub async fn evaluate(
    req: HttpRequest,
    id: web::Path<String>,
    body: web::Json<QualityEvaluateRequest>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.evaluate(&actor, &id, body.into_inner()).await?))
}
pub async fn observations(
    req: HttpRequest,
    id: web::Path<String>,
    q: web::Query<PageQuery>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            svc.list_observations(&actor, &id, q.limit, q.offset)
                .await?,
        ))
}
pub async fn alerts(
    req: HttpRequest,
    id: web::Path<String>,
    q: web::Query<AlertQuery>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            svc.list_alerts(&actor, &id, q.status.clone(), q.limit, q.offset)
                .await?,
        ))
}
pub async fn acknowledge(
    req: HttpRequest,
    id: web::Path<String>,
    body: web::Json<QualityAckRequest>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.acknowledge(&actor, &id, body.into_inner()).await?))
}
pub async fn events(
    req: HttpRequest,
    id: web::Path<String>,
    q: web::Query<PageQuery>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.list_events(&actor, &id, q.limit, q.offset).await?))
}
pub async fn notices(
    req: HttpRequest,
    q: web::Query<NoticeQuery>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            svc.list_notices(&actor, q.unread_only, q.limit, q.offset)
                .await?,
        ))
}
pub async fn read_notice(
    req: HttpRequest,
    id: web::Path<String>,
    svc: web::Data<QualityAlertsService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(svc.mark_notice_read(&actor, &id).await?))
}
