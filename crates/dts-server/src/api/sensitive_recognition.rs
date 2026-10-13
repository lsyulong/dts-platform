use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        product::{ProductService, ResourceAction},
        product_jobs::{JobListQuery, ProductJobService},
        sensitive_recognition::*,
    },
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::json;

fn service(req: &HttpRequest) -> Result<&web::Data<SensitiveRecognitionService>, AppError> {
    req.app_data::<web::Data<SensitiveRecognitionService>>()
        .ok_or_else(|| AppError::Internal("敏感识别服务未配置".into()))
}
pub async fn list(
    req: HttpRequest,
    query: web::Query<RecognitionListQuery>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "sensitive_data:read").await?;
    Ok(HttpResponse::Ok().json(service(&req)?.findings(&actor, query.into_inner()).await?))
}
pub async fn get(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "sensitive_data:read").await?;
    Ok(HttpResponse::Ok().json(service(&req)?.finding(&actor, &path).await?))
}
pub async fn review(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<RecognitionReviewRequest>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::require_permission(&req, "sensitive_data:write").await?;
    let (id, action) = path.into_inner();
    let confirm = match action.as_str() {
        "confirm" => true,
        "dismiss" => false,
        _ => return Err(AppError::NotFound("发现审核动作不存在".into())),
    };
    Ok(HttpResponse::Ok().json(
        service(&req)?
            .review(&actor, &id, confirm, body.into_inner())
            .await?,
    ))
}
pub(super) async fn action(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    action: &str,
    body: ResourceAction,
    resources: &ProductService,
) -> Result<HttpResponse, AppError> {
    let sensitive = service(req)?;
    match action {
        "validate"=>Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":sensitive.validate_saved(actor,kind,id,body.expected_version).await?}))),
        "publish"=>Ok(HttpResponse::Ok().json(sensitive.publish(actor,kind,id,body.expected_version).await?)),
        "withdraw"=>Ok(HttpResponse::Ok().json(sensitive.withdraw(actor,kind,id,body.expected_version).await?)),
        "execute" if kind=="recognition_task"=>{
            let mut payload=body.payload.as_object().cloned().ok_or_else(||AppError::BadRequest("识别运行参数必须为对象".into()))?;
            if payload.contains_key("expected_version"){return Err(AppError::BadRequest("识别参数不得重复提供版本".into()));}
            payload.insert("expected_version".into(),json!(body.expected_version));
            let request:RecognitionRunRequest=serde_json::from_value(serde_json::Value::Object(payload)).map_err(|_|AppError::BadRequest("识别任务运行参数无效".into()))?;
            let submitted=sensitive.enqueue(actor,id,request).await?;
            let jobs=req.app_data::<web::Data<ProductJobService>>().ok_or_else(||AppError::Internal("识别运行服务未配置".into()))?;
            let result=jobs.submission_view(actor,submitted).await?;
            Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/security/recognition-tasks"})))
        },
        "cancel" if kind=="recognition_task"=>{
            let source=resources.get(actor,kind,id).await?;
            if source.version!=body.expected_version{return Err(AppError::Conflict("识别定义版本已改变，请刷新后取消".into()));}
            let jobs=req.app_data::<web::Data<ProductJobService>>().ok_or_else(||AppError::Internal("识别运行服务未配置".into()))?;
            for status in [dts_backend::repo::product_job::ProductJobStatus::Queued,dts_backend::repo::product_job::ProductJobStatus::Running]{
                let page=jobs.list(actor,JobListQuery{source_kind:Some(kind.into()),source_id:Some(id.into()),status:Some(status),limit:Some(1),offset:Some(0)}).await?;
                if let Some(job)=page.data.first(){let result=jobs.cancel(actor,&job.id).await?;return Ok(HttpResponse::Ok().json(json!({"resource":source,"result":result,"run_id":result.id})));}
            }
            Err(AppError::Conflict("当前识别任务没有可取消的运行".into()))
        },
        _=>Err(AppError::BadRequest("敏感识别定义不支持此操作".into())),
    }
}
