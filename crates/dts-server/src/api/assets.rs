use actix_web::{HttpRequest, HttpResponse, web};
use dts_backend::{
    ProductResourceFilter,
    repo::assets::{AssetUsageRequest, AssetView},
};
use serde_json::{Value, json};

use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{assets::AssetsService, product::ListFilters},
};

pub(super) fn service(req: &HttpRequest) -> Result<&web::Data<AssetsService>, AppError> {
    req.app_data::<web::Data<AssetsService>>()
        .ok_or_else(|| AppError::Internal("资产服务未配置".into()))
}

pub(super) fn resource(view: AssetView) -> Result<Value, AppError> {
    let mut resource = serde_json::to_value(view.resource)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    resource["asset_state"] = json!({"publication":view.publication,"source_valid":view.source_valid,
        "stale":view.stale,"invalid_reason":view.invalid_reason,"current_source_version":view.current_source_version});
    Ok(resource)
}

pub(super) async fn list(
    req: &HttpRequest,
    actor: &AuthIdentity,
    filters: ListFilters,
) -> Result<HttpResponse, AppError> {
    if filters.limit.is_some_and(|limit| limit == 0 || limit > 100)
        || filters
            .q
            .as_ref()
            .is_some_and(|q| q.len() > 256 || q.chars().any(char::is_control))
    {
        return Err(AppError::BadRequest("资产分页或搜索参数无效".into()));
    }
    let page = service(req)?
        .list(
            actor,
            ProductResourceFilter {
                tenant_id: actor.tenant_id.clone(),
                kind: "asset_entry".into(),
                q: filters.q,
                status: filters.status,
                limit: filters.limit.unwrap_or(20),
                offset: filters.offset.unwrap_or(0),
                created_by: None,
            },
        )
        .await?;
    let data = page
        .data
        .into_iter()
        .map(resource)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HttpResponse::Ok().json(json!({"data":data,"total":page.total})))
}

pub async fn usage(
    req: HttpRequest,
    body: web::Json<AssetUsageRequest>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "assets:read").await?;
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .analyze_usage(&actor, body.into_inner())
            .await?,
    ))
}
