use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth, models::error::AppError, services::tag_assignments::TagAssignmentService,
};

#[derive(Deserialize)]
pub struct TagAssignmentQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}

pub async fn list(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<TagAssignmentQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:read").await?;
    let service = req
        .app_data::<web::Data<TagAssignmentService>>()
        .ok_or_else(|| AppError::Internal("标签绑定服务未配置".into()))?;
    Ok(HttpResponse::Ok().json(
        service
            .list(
                &actor,
                &path.into_inner(),
                query.limit.unwrap_or(50),
                query.offset.unwrap_or(0),
            )
            .await?,
    ))
}
