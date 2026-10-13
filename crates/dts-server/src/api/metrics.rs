use crate::{
    models::{auth::AuthIdentity, error::AppError},
    services::{
        metrics::{MetricRunRequest, MetricsService},
        product::{ProductService, ResourceAction},
        product_jobs::ProductJobService,
    },
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::{Value, json};

pub(super) async fn action(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    action: &str,
    body: ResourceAction,
    resources: &ProductService,
) -> Result<HttpResponse, AppError> {
    let metrics = req
        .app_data::<web::Data<MetricsService>>()
        .ok_or_else(|| AppError::Internal("指标服务未配置".into()))?;
    match action {
        "validate" => Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":metrics.validate_saved(actor,kind,id,body.expected_version).await?}))),
        "publish" => Ok(HttpResponse::Ok().json(metrics.publish(actor,kind,id,body.expected_version).await?)),
        "withdraw" => Ok(HttpResponse::Ok().json(metrics.withdraw(actor,kind,id,body.expected_version).await?)),
        "calculate" => {
            let mut payload = body.payload.as_object().cloned().ok_or_else(|| AppError::BadRequest("指标计算参数必须为对象".into()))?;
            if payload.contains_key("expected_version") { return Err(AppError::BadRequest("计算参数不得重复提供版本".into())); }
            payload.insert("expected_version".into(),json!(body.expected_version));
            let request: MetricRunRequest = serde_json::from_value(Value::Object(payload)).map_err(|_| AppError::BadRequest("指标计算窗口、参考时刻或参数无效".into()))?;
            let submitted = metrics.enqueue(actor,kind,id,request).await?;
            let jobs = req.app_data::<web::Data<ProductJobService>>().ok_or_else(|| AppError::Internal("指标运行服务未配置".into()))?;
            let result = jobs.submission_view(actor,submitted).await?;
            Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":crate::services::product::catalog::module(kind)?.path})))
        },
        _ => Err(AppError::BadRequest("指标定义不支持此操作".into())),
    }
}
