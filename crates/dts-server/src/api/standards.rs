use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        product::{ProductService, ResourceAction},
        standards::*,
    },
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::{Value, json};

pub async fn revisions(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    query: web::Query<StandardRevisionsQuery>,
    svc: web::Data<StandardGovernanceService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "standards:read").await?;
    let (kind, id) = path.into_inner();
    Ok(HttpResponse::Ok().json(
        svc.revisions(&actor, &kind, &id, query.into_inner())
            .await?,
    ))
}
pub async fn bindings(
    req: HttpRequest,
    path: web::Path<String>,
    svc: web::Data<StandardGovernanceService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "models:read").await?;
    Ok(HttpResponse::Ok().json(json!({"bindings":svc.bindings(&actor,&path).await?})))
}
pub async fn bind_model(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ModelStandardsRequest>,
    svc: web::Data<StandardGovernanceService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "models:write").await?;
    Ok(HttpResponse::Ok().json(svc.bind_model(&actor, &path, body.into_inner()).await?))
}
pub async fn inspect(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<InspectModelStandardsRequest>,
    svc: web::Data<StandardGovernanceService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "standards:execute").await?;
    Ok(HttpResponse::Ok().json(svc.inspect(&actor, &path, body.into_inner()).await?))
}
pub(super) async fn action(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    action: &str,
    request: ResourceAction,
) -> Result<HttpResponse, AppError> {
    let svc = req
        .app_data::<web::Data<StandardGovernanceService>>()
        .ok_or_else(|| AppError::Internal("标准版本服务未配置".into()))?;
    match action {
        "validate" => {
            let definition = svc
                .validate_saved(actor, kind, id, request.expected_version)
                .await?;
            let resources = req
                .app_data::<web::Data<ProductService>>()
                .ok_or_else(|| AppError::Internal("产品定义服务未配置".into()))?;
            let resource = resources.get(actor, kind, id).await?;
            if resource.version != request.expected_version {
                return Err(AppError::Conflict("定义验证期间版本已改变".into()));
            }
            let mut result = serde_json::to_value(definition)
                .map_err(|_| AppError::Internal("标准校验结果编码失败".into()))?;
            let payload = request
                .payload
                .as_object()
                .ok_or_else(|| AppError::BadRequest("校验参数必须为对象".into()))?;
            if kind == "naming_standard" && !payload.is_empty() {
                if payload.len() != 1
                    || !payload
                        .keys()
                        .all(|key| matches!(key.as_str(), "sample" | "name"))
                {
                    return Err(AppError::BadRequest(
                        "命名校验仅接受一个 sample 或 name 字段".into(),
                    ));
                }
                let name = crate::services::product_domain::warehouse::action(
                    kind,
                    &resource.properties,
                    "validate",
                    &request.payload,
                )?
                .ok_or_else(|| AppError::Internal("命名校验器未配置".into()))?;
                result["definition_valid"] = json!(true);
                result["valid"] = name["valid"].clone();
                result["name_validation"] = name;
            } else {
                empty_payload(&request.payload)?;
            }
            Ok(HttpResponse::Ok().json(json!({"resource":resource,"result":result})))
        },
        "publish" => {
            empty_payload(&request.payload)?;
            Ok(HttpResponse::Ok().json(
                svc.publish(actor, kind, id, request.expected_version)
                    .await?,
            ))
        },
        "revise" => {
            empty_payload(&request.payload)?;
            Ok(HttpResponse::Ok().json(
                svc.revise(actor, kind, id, request.expected_version)
                    .await?,
            ))
        },
        "withdraw" => {
            let mut payload = request
                .payload
                .as_object()
                .cloned()
                .ok_or_else(|| AppError::BadRequest("撤回载荷必须为对象".into()))?;
            if payload.contains_key("expected_version") {
                return Err(AppError::BadRequest("载荷不能重复声明定义版本".into()));
            }
            payload.insert("expected_version".into(), json!(request.expected_version));
            let body: WithdrawStandardRequest = serde_json::from_value(Value::Object(payload))
                .map_err(|_| AppError::BadRequest("撤回需指定发布版本、状态版本与原因".into()))?;
            Ok(HttpResponse::Ok().json(svc.withdraw(actor, kind, id, body).await?))
        },
        _ => Err(AppError::BadRequest("标准版本服务不支持此动作".into())),
    }
}
fn empty_payload(payload: &Value) -> Result<(), AppError> {
    if !payload.as_object().is_some_and(|map| map.is_empty()) {
        return Err(AppError::BadRequest(
            "发布及修订不能注入客户端版本证据".into(),
        ));
    }
    Ok(())
}
