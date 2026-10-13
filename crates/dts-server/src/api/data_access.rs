use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use serde_json::json;

use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_access::DataAccessService,
        product::{ListFilters, ResourceAction},
    },
};

pub fn service(req: &HttpRequest) -> Result<&web::Data<DataAccessService>, AppError> {
    req.app_data::<web::Data<DataAccessService>>()
        .ok_or_else(|| AppError::Internal("数据权限服务未配置".into()))
}

pub async fn resource_list(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    filters: ListFilters,
) -> Result<HttpResponse, AppError> {
    if filters.q.as_ref().is_some_and(|q| !q.is_empty()) {
        return Err(AppError::BadRequest(
            "权限申请与审批列表支持状态筛选，请清除搜索词".into(),
        ));
    }
    let svc = service(req)?;
    let limit = filters.limit.unwrap_or(20);
    let offset = filters.offset.unwrap_or(0);
    let page = if kind == "data_access_approval" {
        svc.list_approvals(
            actor,
            filters.status.filter(|status| !status.is_empty()),
            limit,
            offset,
        )
        .await?
    } else {
        svc.list_requests(
            actor,
            filters.status.filter(|status| !status.is_empty()),
            limit,
            offset,
        )
        .await?
    };
    Ok(HttpResponse::Ok().json(page))
}

#[derive(Deserialize)]
pub struct PageQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub async fn approvals(
    req: HttpRequest,
    filters: web::Query<PageQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "data_security:read").await?;
    auth::require_permission(&req, "data_security:approve").await?;
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .list_approvals(
                &actor,
                filters.status.clone().filter(|status| !status.is_empty()),
                filters.limit.unwrap_or(20),
                filters.offset.unwrap_or(0),
            )
            .await?,
    ))
}

pub async fn audit(
    req: HttpRequest,
    filters: web::Query<PageQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "data_security:read").await?;
    if filters
        .status
        .as_ref()
        .is_some_and(|status| !status.is_empty())
    {
        return Err(AppError::BadRequest(
            "权限审计当前支持分页，请清除状态筛选".into(),
        ));
    }
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .list_audit(
                &actor,
                filters.limit.unwrap_or(20),
                filters.offset.unwrap_or(0),
            )
            .await?,
    ))
}

pub async fn grants(req: HttpRequest) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "data_security:read").await?;
    Ok(HttpResponse::Ok().json(json!({"data":service(&req)?.list_active_grants(&actor).await?})))
}

pub async fn decision(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<ResourceAction>,
) -> Result<HttpResponse, AppError> {
    let (id, action) = path.into_inner();
    let permission = if matches!(action.as_str(), "approve" | "reject" | "revoke") {
        "data_security:approve"
    } else {
        "data_security:write"
    };
    let actor = auth::require_permission(&req, permission).await?;
    let result = service(&req)?
        .action(&actor, &id, &action, body.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(json!({"resource":result.request,"result":{"grant":result.grant,"decision_id":result.decision_id,"replayed":result.replayed}})))
}
