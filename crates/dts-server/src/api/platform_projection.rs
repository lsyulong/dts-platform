use crate::{
    middleware::auth,
    models::error::AppError,
    services::platform_projection::{PlatformProjectionService, StatisticsView},
};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use dts_backend::repo::platform_projection::WorkSource;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemsQuery {
    pub source: Option<WorkSource>,
    #[serde(default = "limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}
fn limit() -> u32 {
    20
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatisticsQuery {
    pub view: StatisticsView,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
pub async fn work_items(
    req: HttpRequest,
    q: web::Query<WorkItemsQuery>,
    svc: web::Data<PlatformProjectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(svc.work_items(&actor, q.source, q.limit, q.offset).await?))
}
pub async fn statistics(
    req: HttpRequest,
    q: web::Query<StatisticsQuery>,
    svc: web::Data<PlatformProjectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(svc.statistics(&actor, q.view, q.start, q.end).await?))
}
