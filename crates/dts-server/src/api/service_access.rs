use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        product::{ListFilters, ResourceAction, ResourceInput, ResourceUpdate},
        service_access::ServiceAccessService,
    },
};

pub(super) fn service(req: &HttpRequest) -> Result<&web::Data<ServiceAccessService>, AppError> {
    req.app_data::<web::Data<ServiceAccessService>>()
        .ok_or_else(|| AppError::Internal("服务应用授权模块未配置".into()))
}
pub(super) fn handles(kind: &str) -> bool {
    matches!(kind, "service_application" | "service_subscription")
}

pub(super) async fn list(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    filters: ListFilters,
) -> Result<HttpResponse, AppError> {
    let service = service(req)?;
    let result = if kind == "service_application" {
        service
            .list_applications(
                actor,
                filters.q,
                filters.status,
                filters.limit.unwrap_or(20),
                filters.offset.unwrap_or(0),
            )
            .await?
    } else {
        service
            .list_subscriptions(
                actor,
                false,
                filters.q,
                filters.status,
                filters.limit.unwrap_or(20),
                filters.offset.unwrap_or(0),
            )
            .await?
    };
    Ok(HttpResponse::Ok().json(result))
}
pub(super) async fn get(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
) -> Result<HttpResponse, AppError> {
    let resource = if kind == "service_application" {
        service(req)?.get_application(actor, id).await?
    } else {
        service(req)?.get_subscription(actor, id).await?
    };
    Ok(HttpResponse::Ok().json(resource))
}
pub(super) async fn create(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    input: ResourceInput,
) -> Result<HttpResponse, AppError> {
    let resource = if kind == "service_application" {
        service(req)?.save_application(actor, input).await?
    } else {
        service(req)?.save_subscription(actor, input).await?
    };
    Ok(HttpResponse::Created().json(resource))
}
pub(super) async fn update(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    input: ResourceUpdate,
) -> Result<HttpResponse, AppError> {
    let resource = if kind == "service_application" {
        service(req)?.update_application(actor, id, input).await?
    } else {
        service(req)?.update_subscription(actor, id, input).await?
    };
    Ok(HttpResponse::Ok().json(resource))
}
pub(super) async fn action(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    action: &str,
    input: ResourceAction,
) -> Result<HttpResponse, AppError> {
    if kind == "service_subscription" {
        if matches!(action, "approve" | "reject" | "revoke") {
            auth::require_permission(req, "services:approve").await?;
        }
        let result = service(req)?
            .subscription_action(actor, id, action, input)
            .await?;
        return Ok(HttpResponse::Ok().json(json!({"resource":result.subscription,"result":{"decision_id":result.decision_id,"replayed":result.replayed}})));
    }
    if action != "archive"
        || input
            .payload
            .as_object()
            .is_some_and(|payload| !payload.is_empty())
        || (!input.payload.is_null() && !input.payload.is_object())
    {
        return Err(AppError::BadRequest(
            "应用仅支持明确归档操作，不接受自定义结果".into(),
        ));
    }
    Ok(HttpResponse::Ok().json(json!({"resource":service(req)?.archive_application(actor,id,input.expected_version).await?})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
pub async fn credentials(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<PageQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "services:read").await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(
            service(&req)?
                .list_credentials(
                    &actor,
                    &path,
                    query.limit.unwrap_or(20),
                    query.offset.unwrap_or(0),
                )
                .await?,
        ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCredential {
    pub expected_version: i64,
    pub idempotency_key: String,
    pub expires_at: Option<DateTime<Utc>>,
}
pub async fn issue(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<IssueCredential>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "services:write").await?;
    let result=service(&req)?.issue_credential(&actor,&path,ResourceAction {expected_version:body.expected_version,payload:json!({"idempotency_key":body.idempotency_key,"expires_at":body.expires_at})}).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("Pragma", "no-cache"))
        .insert_header(("Referrer-Policy", "no-referrer"))
        .json(result))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeCredential {
    pub expected_version: i64,
    pub idempotency_key: String,
}
pub async fn revoke(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<RevokeCredential>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "services:write").await?;
    let (application, credential) = path.into_inner();
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .revoke_credential(
                &actor,
                &application,
                &credential,
                body.expected_version,
                &body.idempotency_key,
            )
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
pub async fn approvals(
    req: HttpRequest,
    query: web::Query<ApprovalQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "services:approve").await?;
    let query = query.into_inner();
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .list_subscriptions(
                &actor,
                true,
                query.q,
                query.status,
                query.limit.unwrap_or(20),
                query.offset.unwrap_or(0),
            )
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallsQuery {
    pub application_id: Option<String>,
    pub api_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}
pub async fn calls(
    req: HttpRequest,
    query: web::Query<CallsQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "services:read").await?;
    let query = query.into_inner();
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .list_calls(
                &actor,
                query.application_id,
                query.api_id,
                query.limit.unwrap_or(20),
                query.offset.unwrap_or(0),
            )
            .await?,
    ))
}
