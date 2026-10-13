use actix_web::{HttpRequest, HttpResponse, web};

use crate::{
    middleware::auth, models::error::AppError,
    services::development_schedules::DevelopmentScheduleService,
};

pub async fn status(
    req: HttpRequest,
    path: web::Path<String>,
    service: web::Data<DevelopmentScheduleService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "development:read").await?;
    Ok(HttpResponse::Ok().json(service.status(&actor, &path).await?))
}
