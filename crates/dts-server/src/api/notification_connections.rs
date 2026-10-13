use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{
    middleware::auth,
    models::error::AppError,
    services::notification_connections::{
        CreateNotificationConnectionRequest, NotificationConnectionService,
        SetNotificationConnectionEnabledRequest, UpdateNotificationConnectionRequest,
    },
};

#[derive(Deserialize)]
pub struct DeleteQuery {
    expected_version: i64,
}

pub async fn list(
    req: HttpRequest,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:read").await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"data":service.list(&actor.tenant_id).await?})))
}
pub async fn get(
    req: HttpRequest,
    id: web::Path<String>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:read").await?;
    Ok(HttpResponse::Ok().json(service.get(&actor.tenant_id, &id).await?))
}
pub async fn create(
    req: HttpRequest,
    body: web::Json<CreateNotificationConnectionRequest>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:write").await?;
    Ok(HttpResponse::Created().json(
        service
            .create(&actor.tenant_id, &actor.sub, body.into_inner())
            .await?,
    ))
}
pub async fn update(
    req: HttpRequest,
    id: web::Path<String>,
    body: web::Json<UpdateNotificationConnectionRequest>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:write").await?;
    Ok(HttpResponse::Ok().json(
        service
            .update(&actor.tenant_id, &id, &actor.sub, body.into_inner())
            .await?,
    ))
}
pub async fn enabled(
    req: HttpRequest,
    id: web::Path<String>,
    body: web::Json<SetNotificationConnectionEnabledRequest>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:write").await?;
    Ok(HttpResponse::Ok().json(
        service
            .set_enabled(&actor.tenant_id, &id, &actor.sub, body.into_inner())
            .await?,
    ))
}
pub async fn delete(
    req: HttpRequest,
    id: web::Path<String>,
    query: web::Query<DeleteQuery>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:write").await?;
    service
        .delete(&actor.tenant_id, &id, query.expected_version, &actor.sub)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
pub async fn test(
    req: HttpRequest,
    id: web::Path<String>,
    service: web::Data<NotificationConnectionService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "support:write").await?;
    Ok(HttpResponse::Ok().json(service.test(&actor.tenant_id, &id, &actor.sub).await?))
}
