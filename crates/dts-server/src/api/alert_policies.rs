use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth, models::error::AppError, services::alert_policies::AlertPolicyService,
};

#[derive(Deserialize)]
pub struct AlertPolicyEvaluationQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}

pub async fn evaluations(
    req: HttpRequest,
    id: web::Path<String>,
    query: web::Query<AlertPolicyEvaluationQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:read").await?;
    let service = req
        .app_data::<web::Data<AlertPolicyService>>()
        .ok_or_else(|| AppError::Internal("告警策略服务未配置".into()))?;
    Ok(HttpResponse::Ok().json(
        service
            .list(
                &actor,
                &id,
                query.limit.unwrap_or(50),
                query.offset.unwrap_or(0),
            )
            .await?,
    ))
}
