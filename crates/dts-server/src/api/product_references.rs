use crate::{middleware::auth, models::error::AppError, services::product::ProductService};
use actix_web::{HttpRequest, HttpResponse, web};
use dts_backend::repo::UserRepo;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserDirectoryQuery {
    pub resource_kind: String,
    #[serde(default)]
    pub q: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn users(
    req: HttpRequest,
    query: web::Query<UserDirectoryQuery>,
    repo: web::Data<Arc<dyn UserRepo>>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(
        &req,
        &ProductService::permission(&query.resource_kind, false)?,
    )
    .await?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || query.q.len() > 128 || query.q.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "用户目录筛选限128字节，每页1至100人".into(),
        ));
    }
    Ok(HttpResponse::Ok().json(
        repo.list_references(
            &actor.tenant_id,
            query.q.trim(),
            limit,
            query.offset.unwrap_or(0),
        )
        .await?,
    ))
}
