use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth, models::error::AppError, services::message_delivery::MessageService,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxQuery {
    #[serde(default)]
    pub unread_only: bool,
    #[serde(default = "limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}
fn limit() -> u32 {
    20
}

pub async fn inbox(
    req: HttpRequest,
    query: web::Query<InboxQuery>,
    service: web::Data<MessageService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(
        service
            .inbox(
                &actor.tenant_id,
                &actor.sub,
                query.unread_only,
                query.limit,
                query.offset,
            )
            .await?,
    ))
}

pub async fn mark_read(
    req: HttpRequest,
    id: web::Path<String>,
    service: web::Data<MessageService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(service.mark_read(&actor.tenant_id, &actor.sub, &id).await?))
}
