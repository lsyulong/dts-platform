use crate::{
    middleware::auth, models::error::AppError, services::system_content::SystemContentService,
};
use actix_web::{HttpRequest, HttpResponse, web};

pub async fn visible(
    req: HttpRequest,
    kind: web::Path<String>,
    service: web::Data<SystemContentService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"data":service.visible(&actor,&kind).await?})))
}
