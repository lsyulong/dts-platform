use crate::{middleware::auth, models::error::AppError, services::data_quality::QualityService};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultsQuery {
    pub task_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn results(
    req: HttpRequest,
    query: web::Query<ResultsQuery>,
    service: web::Data<QualityService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "quality:read").await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(AppError::BadRequest("分页条数须为1至100".into()));
    }
    Ok(HttpResponse::Ok().json(
        service
            .results(
                &actor,
                query.task_id.clone(),
                limit,
                query.offset.unwrap_or(0),
            )
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRequest {
    pub task_id: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

pub async fn report(
    req: HttpRequest,
    body: web::Json<ReportRequest>,
    service: web::Data<QualityService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "quality:read").await?;
    if body.period_start >= body.period_end
        || body.period_end - body.period_start > chrono::Duration::days(366)
    {
        return Err(AppError::BadRequest(
            "报告时间范围须为1年以内且结束晚于开始".into(),
        ));
    }
    Ok(HttpResponse::Ok().json(
        service
            .report(
                &actor,
                body.task_id.clone(),
                body.period_start,
                body.period_end,
            )
            .await?,
    ))
}
