use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::{middleware::auth, models::error::AppError, services::product::*};

pub async fn schemas(req: HttpRequest) -> Result<HttpResponse, AppError> {
    auth::authenticate_request(&req).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"data":ProductService::schemas()?})))
}

pub async fn list(
    req: HttpRequest,
    path: web::Path<String>,
    filters: web::Query<ListFilters>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, &ProductService::permission(&path, false)?).await?;
    if super::service_access::handles(&path) {
        return super::service_access::list(&req, &actor, &path, filters.into_inner()).await;
    }
    if path.as_str() == "asset_entry" {
        return super::assets::list(&req, &actor, filters.into_inner()).await;
    }
    if matches!(
        path.as_str(),
        "data_access_request" | "data_access_approval"
    ) {
        return super::data_access::resource_list(&req, &actor, &path, filters.into_inner()).await;
    }
    Ok(HttpResponse::Ok().json(svc.list(&actor, &path, filters.into_inner()).await?))
}

pub async fn get(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let (kind, id) = path.into_inner();
    let actor = auth::require_permission(&req, &ProductService::permission(&kind, false)?).await?;
    if super::service_access::handles(&kind) {
        return super::service_access::get(&req, &actor, &kind, &id).await;
    }
    if kind == "asset_entry" {
        return Ok(HttpResponse::Ok().json(super::assets::resource(
            super::assets::service(&req)?.get(&actor, &id).await?,
        )?));
    }
    if kind == "data_access_request" {
        return Ok(HttpResponse::Ok().json(
            super::data_access::service(&req)?
                .get_request(&actor, &id)
                .await?,
        ));
    }
    Ok(HttpResponse::Ok().json(svc.get(&actor, &kind, &id).await?))
}

pub async fn create(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ResourceInput>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, &ProductService::permission(&path, true)?).await?;
    if super::service_access::handles(&path) {
        return super::service_access::create(&req, &actor, &path, body.into_inner()).await;
    }
    if crate::services::assets::AssetsService::handles(&path) {
        return Ok(HttpResponse::Created().json(
            super::assets::service(&req)?
                .create(&actor, &path, body.into_inner())
                .await?,
        ));
    }
    if matches!(path.as_str(), "masking_rule" | "user_group") {
        req.app_data::<web::Data<crate::services::data_masking::MaskingService>>()
            .ok_or_else(|| AppError::Internal("脱敏服务未配置".into()))?
            .validate_definition(&actor, &path, &body.properties)
            .await?;
    }
    if path.as_str() == "metadata_lineage" {
        let lineage = req
            .app_data::<web::Data<crate::services::metadata_lineage::MetadataLineageService>>()
            .ok_or_else(|| AppError::Internal("元数据血缘服务未配置".into()))?;
        return Ok(HttpResponse::Created().json(lineage.create(&actor, body.into_inner()).await?));
    }
    if path.as_str() == "data_access_request" {
        return Ok(HttpResponse::Created().json(
            super::data_access::service(&req)?
                .save_request(&actor, body.into_inner())
                .await?,
        ));
    }
    if crate::services::master_data::MasterDataService::handles(&path) {
        let service = req
            .app_data::<web::Data<crate::services::master_data::MasterDataService>>()
            .ok_or_else(|| AppError::Internal("主数据服务未配置".into()))?;
        return Ok(
            HttpResponse::Created().json(service.create(&actor, &path, body.into_inner()).await?)
        );
    }
    Ok(HttpResponse::Created().json(svc.create(&actor, &path, body.into_inner()).await?))
}

pub async fn update(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<ResourceUpdate>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let (kind, id) = path.into_inner();
    let actor = auth::require_permission(&req, &ProductService::permission(&kind, true)?).await?;
    if super::service_access::handles(&kind) {
        return super::service_access::update(&req, &actor, &kind, &id, body.into_inner()).await;
    }
    if crate::services::assets::AssetsService::handles(&kind) {
        return Ok(HttpResponse::Ok().json(
            super::assets::service(&req)?
                .update(&actor, &kind, &id, body.into_inner())
                .await?,
        ));
    }
    if matches!(kind.as_str(), "masking_rule" | "user_group") {
        req.app_data::<web::Data<crate::services::data_masking::MaskingService>>()
            .ok_or_else(|| AppError::Internal("脱敏服务未配置".into()))?
            .validate_definition(&actor, &kind, &body.properties)
            .await?;
    }
    if kind == "metadata_lineage" {
        let lineage = req
            .app_data::<web::Data<crate::services::metadata_lineage::MetadataLineageService>>()
            .ok_or_else(|| AppError::Internal("元数据血缘服务未配置".into()))?;
        return Ok(HttpResponse::Ok().json(lineage.update(&actor, &id, body.into_inner()).await?));
    }
    if kind == "data_access_request" {
        return Ok(HttpResponse::Ok().json(
            super::data_access::service(&req)?
                .update_request(&actor, &id, body.into_inner())
                .await?,
        ));
    }
    if crate::services::master_data::MasterDataService::handles(&kind) {
        let service = req
            .app_data::<web::Data<crate::services::master_data::MasterDataService>>()
            .ok_or_else(|| AppError::Internal("主数据服务未配置".into()))?;
        return Ok(HttpResponse::Ok().json(
            service
                .update(&actor, &kind, &id, body.into_inner())
                .await?,
        ));
    }
    Ok(HttpResponse::Ok().json(svc.update(&actor, &kind, &id, body.into_inner()).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteVersion {
    pub expected_version: i64,
}

pub async fn delete(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    query: web::Query<DeleteVersion>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let (kind, id) = path.into_inner();
    let actor = auth::require_permission(&req, &ProductService::permission(&kind, true)?).await?;
    if super::service_access::handles(&kind) {
        return Err(AppError::Conflict(
            "应用与订阅须通过归档或撤销流程处理，历史记录不能删除".into(),
        ));
    }
    if crate::services::assets::AssetsService::handles(&kind) {
        super::assets::service(&req)?
            .delete(&actor, &kind, &id, query.expected_version)
            .await?;
        return Ok(HttpResponse::NoContent().finish());
    }
    if crate::services::master_data::MasterDataService::handles(&kind) {
        let service = req
            .app_data::<web::Data<crate::services::master_data::MasterDataService>>()
            .ok_or_else(|| AppError::Internal("主数据服务未配置".into()))?;
        service
            .delete(&actor, &kind, &id, query.expected_version)
            .await?;
        return Ok(HttpResponse::NoContent().finish());
    }
    svc.delete(&actor, &kind, &id, query.expected_version)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn audit(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let (kind, id) = path.into_inner();
    let actor = auth::require_permission(&req, &ProductService::permission(&kind, false)?).await?;
    if kind == "asset_entry" {
        super::assets::service(&req)?.get(&actor, &id).await?;
    }
    if super::service_access::handles(&kind) {
        super::service_access::get(&req, &actor, &kind, &id).await?;
    }
    Ok(HttpResponse::Ok().json(svc.audit(&actor, &kind, &id).await?))
}

pub async fn action(
    req: HttpRequest,
    path: web::Path<(String, String, String)>,
    body: web::Json<ResourceAction>,
    svc: web::Data<ProductService>,
) -> Result<HttpResponse, AppError> {
    let (kind, id, action) = path.into_inner();
    if kind == "data_access_request" {
        return super::data_access::decision(req, web::Path::from((id, action)), body).await;
    }
    if kind == "service_subscription" {
        let permission = if matches!(action.as_str(), "approve" | "reject" | "revoke") {
            "services:approve"
        } else {
            "services:write"
        };
        let actor = auth::require_permission(&req, permission).await?;
        return super::service_access::action(&req, &actor, &kind, &id, &action, body.into_inner())
            .await;
    }
    let execution = matches!(
        (kind.as_str(), action.as_str()),
        (
            "atomic_metric" | "derived_metric" | "composite_metric",
            "calculate"
        ) | (
            "sql_document"
                | "development_workflow"
                | "bi_report"
                | "data_api"
                | "quality_task"
                | "metadata_collection"
                | "recognition_task",
            "execute"
        ) | ("development_workflow" | "recognition_task", "cancel")
            | ("master_code_rule", "allocate")
    );
    let mutating = !(execution
        || matches!(kind.as_str(), "metadata_lineage" | "asset_map") && action == "analyze"
        || dts_backend::repo::standard_governance::STANDARD_KINDS.contains(&kind.as_str())
            && action == "validate"
        || kind == "development_config" && action == "validate");
    let actor =
        auth::require_permission(&req, &ProductService::permission(&kind, mutating)?).await?;
    if execution {
        let domain = crate::services::product::catalog::scope(
            crate::services::product::catalog::module(&kind)?,
        );
        auth::require_permission(&req, &format!("{domain}:execute")).await?;
    }
    if kind == "system_message" && matches!(action.as_str(), "send" | "retry") {
        let delivery = req
            .app_data::<web::Data<crate::services::message_delivery::MessageService>>()
            .ok_or_else(|| AppError::Internal("站内消息服务未配置".into()))?;
        let key = body
            .payload
            .get("idempotency_key")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("send_{}_{}", id, body.expected_version));
        let batch = delivery
            .send(
                &actor.tenant_id,
                &id,
                body.expected_version,
                &actor.sub,
                &key,
            )
            .await?;
        return Ok(HttpResponse::Ok().json(
            serde_json::json!({"resource":svc.get(&actor,&kind,&id).await?,"result":batch}),
        ));
    }
    if kind == "data_model" && action == "publish" {
        auth::require_permission(&req, "models:execute").await?;
    }
    super::product_actions::execute(&req, &actor, &kind, &id, &action, body.into_inner(), &svc)
        .await
}
