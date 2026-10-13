use crate::{
    middleware::auth,
    models::{auth::AuthIdentity, error::AppError},
    services::{
        data_quality::{QualityRunRequest, QualityService},
        master_data::MasterDataService,
        product::{ProductService, ResourceAction},
        product_jobs::{JobListQuery, ProductJobService, RunRequest},
    },
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::{Value, json};

fn service<T: 'static>(req: &HttpRequest) -> Result<&web::Data<T>, AppError> {
    req.app_data::<web::Data<T>>()
        .ok_or_else(|| AppError::Internal("业务服务未配置".into()))
}

fn key(payload: &Value) -> Result<String, AppError> {
    payload
        .get("idempotency_key")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::BadRequest("本次操作缺少稳定幂等键".into()))
}

pub async fn execute(
    req: &HttpRequest,
    actor: &AuthIdentity,
    kind: &str,
    id: &str,
    action: &str,
    body: ResourceAction,
    resources: &ProductService,
) -> Result<HttpResponse, AppError> {
    if kind == "alert_policy" && matches!(action, "evaluate" | "test_delivery") {
        let request = serde_json::from_value(body.payload)
            .map_err(|_| AppError::BadRequest("告警操作须提供稳定幂等键".into()))?;
        let result = service::<crate::services::alert_policies::AlertPolicyService>(req)?
            .execute(actor, id, body.expected_version, action, request)
            .await?;
        return Ok(HttpResponse::Ok().json(json!({
            "resource":resources.get(actor,kind,id).await?,"result":result
        })));
    }
    if kind == "development_config" {
        use crate::services::runtime_configuration::{
            ConfigurationActionRequest, RuntimeConfigurationService,
        };
        let svc = service::<RuntimeConfigurationService>(req)?;
        if action == "validate" {
            if !body.payload.as_object().is_some_and(|p| p.is_empty()) {
                return Err(AppError::BadRequest("配置校验不接受客户端证据".into()));
            }
            return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":svc.validate(actor,id,body.expected_version).await?})));
        }
        let mut payload = body
            .payload
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("配置操作参数必须为对象".into()))?;
        if payload.contains_key("expected_version") {
            return Err(AppError::BadRequest("不能覆盖配置版本".into()));
        }
        payload.insert("expected_version".into(), json!(body.expected_version));
        let request: ConfigurationActionRequest = serde_json::from_value(Value::Object(payload))
            .map_err(|_| AppError::BadRequest("配置操作须提供版本、原因和稳定幂等键".into()))?;
        let changed = match action {
            "publish" => svc.publish(actor, id, request).await?,
            "withdraw" => svc.withdraw(actor, id, request).await?,
            _ => return Err(AppError::BadRequest("配置不支持此操作".into())),
        };
        let resource = resources.get(actor, kind, id).await?;
        return Ok(
            HttpResponse::Ok().json(json!({"resource":resource,"result":changed,"executed":false}))
        );
    }
    if dts_backend::repo::standard_governance::STANDARD_KINDS.contains(&kind) && action != "convert"
    {
        return super::standards::action(req, actor, kind, id, action, body).await;
    }
    if matches!(kind, "sql_document" | "development_workflow") && action == "save_version" {
        let mut payload = body
            .payload
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("保存版本参数必须为对象".into()))?;
        if payload.contains_key("expected_version") {
            return Err(AppError::BadRequest("不能覆盖定义版本".into()));
        }
        payload.insert("expected_version".into(), json!(body.expected_version));
        let request = serde_json::from_value(Value::Object(payload)).map_err(|_| {
            AppError::BadRequest("保存版本仅接受实际版本、保存原因和稳定幂等键".into())
        })?;
        let result =
            service::<crate::services::development_revisions::DevelopmentRevisionsService>(req)?
                .save(actor, kind, id, request)
                .await?;
        return Ok(HttpResponse::Ok()
            .json(json!({"resource":resources.get(actor,kind,id).await?,"result":result})));
    }
    if crate::services::metrics::handles(kind) {
        return super::metrics::action(req, actor, kind, id, action, body, resources).await;
    }
    if matches!(
        kind,
        "data_classification" | "recognition_rule" | "recognition_task"
    ) {
        return super::sensitive_recognition::action(req, actor, kind, id, action, body, resources)
            .await;
    }
    if super::service_access::handles(kind) {
        return super::service_access::action(req, actor, kind, id, action, body).await;
    }
    if crate::services::assets::AssetsService::handles(kind) {
        return Ok(HttpResponse::Ok().json(
            super::assets::service(req)?
                .action(actor, kind, id, action, body)
                .await?,
        ));
    }
    if kind == "data_api" {
        let api = service::<crate::services::data_services::DataApiService>(req)?;
        return match action {
            "validate" => Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":api.validate_saved(actor,id,body.expected_version).await?}))),
            "publish" => Ok(HttpResponse::Ok().json(api.publish(actor,id,body.expected_version).await?)),
            "withdraw" => Ok(HttpResponse::Ok().json(api.withdraw(actor,id,body.expected_version).await?)),
            "execute" => {
                let result=api.enqueue(actor,id,crate::services::data_services::DataApiRunRequest {
                    expected_version:body.expected_version,idempotency_key:key(&body.payload)?,
                    parameters:body.payload.get("parameters").cloned().unwrap_or_else(||json!({})),
                }).await?;
                let result=service::<ProductJobService>(req)?.submission_view(actor,result).await?;
                Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/services/apis"})))
            },
            _=>Err(AppError::BadRequest("数据 API 不支持此操作".into())),
        };
    }
    if kind == "metadata_lineage" {
        return Ok(HttpResponse::Ok().json(
            service::<crate::services::metadata_lineage::MetadataLineageService>(req)?
                .action(actor, id, action, body)
                .await?,
        ));
    }
    if matches!(kind, "masking_rule" | "user_group") {
        let masking = service::<crate::services::data_masking::MaskingService>(req)?;
        let resource = resources.get(actor, kind, id).await?;
        if resource.version != body.expected_version {
            return Err(AppError::Conflict(
                "脱敏配置版本已变化，请刷新后重试".into(),
            ));
        }
        match action {
            "validate" => return Ok(HttpResponse::Ok().json(json!({"resource":resource,"result":masking.validate_saved(actor,kind,id).await?}))),
            "preview" if kind == "masking_rule" => return Ok(HttpResponse::Ok().json(json!({"resource":resource,"result":masking.preview(actor,id,&body.payload).await?}))),
            "publish" => return Ok(HttpResponse::Ok().json(json!({"resource":masking.publish(actor,kind,id,body.expected_version).await?}))),
            "withdraw" => return Ok(HttpResponse::Ok().json(json!({"resource":masking.withdraw(actor,kind,id,body.expected_version).await?}))),
            "assign_members" if kind == "user_group" => return Ok(HttpResponse::Ok().json(json!({"resource":masking.assign_members(actor,id,body.expected_version,&body.payload).await?}))),
            _ => return Err(AppError::BadRequest("当前脱敏配置不支持此操作".into())),
        }
    }
    if MasterDataService::handles(kind) {
        if matches!(action, "approve" | "reject") {
            auth::require_permission(req, "master_data:approve").await?;
        }
        if action == "allocate" {
            auth::require_permission(req, "master_data:execute").await?;
        }
        return Ok(HttpResponse::Ok().json(
            service::<MasterDataService>(req)?
                .action(actor, kind, id, action, body)
                .await?,
        ));
    }
    if kind == "tag" && matches!(action, "assign" | "unassign") {
        let request = serde_json::from_value(body.payload).map_err(|_| {
            AppError::BadRequest("标签绑定操作须提供目标类型、目标ID和稳定幂等键".into())
        })?;
        let result = service::<crate::services::tag_assignments::TagAssignmentService>(req)?
            .change(actor, id, body.expected_version, action, request)
            .await?;
        return Ok(HttpResponse::Ok()
            .json(json!({"resource":resources.get(actor,kind,id).await?,"result":result})));
    }
    if matches!(kind, "sql_document" | "development_workflow") {
        let jobs = service::<ProductJobService>(req)?;
        match action {
            "register_schedule" if kind == "development_workflow" => {
                let request = serde_json::from_value(body.payload.clone()).map_err(|_| {
                    AppError::BadRequest("注册周期计划须提供变更原因和稳定幂等键".into())
                })?;
                let result = service::<crate::services::development_schedules::DevelopmentScheduleService>(req)?
                    .register(actor,id,body.expected_version,request).await?;
                return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":result})))
            },
            "unschedule" if kind == "development_workflow" => {
                let request = serde_json::from_value(body.payload.clone()).map_err(|_| {
                    AppError::BadRequest("停用周期计划须提供变更原因和稳定幂等键".into())
                })?;
                let result = service::<crate::services::development_schedules::DevelopmentScheduleService>(req)?
                    .unschedule(actor,id,body.expected_version,request).await?;
                return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":result})))
            },
            "validate" => return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":jobs.validate(actor,kind,id,body.expected_version).await?}))),
            "publish" => return Ok(HttpResponse::Ok().json(jobs.publish(actor,kind,id,body.expected_version).await?)),
            "withdraw" => {
                if kind == "development_workflow" {
                    service::<crate::services::development_schedules::DevelopmentScheduleService>(req)?
                        .ensure_can_withdraw(actor, id)
                        .await?;
                }
                return Ok(HttpResponse::Ok().json(jobs.withdraw(actor,kind,id,body.expected_version).await?))
            },
            "execute" => {
                let result = jobs.enqueue(actor,kind,id,RunRequest { expected_version:body.expected_version,idempotency_key:key(&body.payload)?,parameters:body.payload.get("parameters").cloned().unwrap_or_else(||json!({})) }).await?;
                let result = jobs.submission_view(actor, result).await?;
                return Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/development/workflow-runs"})));
            },
            "cancel" => {
                let source = resources.get(actor,kind,id).await?;
                if source.version != body.expected_version { return Err(AppError::Conflict("定义版本已改变，请刷新后取消".into())); }
                for status in [dts_backend::repo::product_job::ProductJobStatus::Queued,dts_backend::repo::product_job::ProductJobStatus::Running] {
                    let page = jobs.list(actor,JobListQuery {source_kind:Some(kind.into()),source_id:Some(id.into()),status:Some(status),limit:Some(1),offset:Some(0)}).await?;
                    if let Some(job) = page.data.first() {
                        let result = jobs.cancel(actor,&job.id).await?;
                        return Ok(HttpResponse::Ok().json(json!({"resource":source,"result":result,"run_id":result.id,"next_path":"/development/workflow-runs"})));
                    }
                }
                return Err(AppError::Conflict("当前定义没有可取消的运行".into()));
            },
            _ => {},
        }
    }
    if kind == "bi_report" {
        let bi = service::<crate::services::bi_report::BiReportService>(req)?;
        match action {
            "validate" => return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":bi.validate_saved(actor,id,body.expected_version).await?}))),
            "publish" => return Ok(HttpResponse::Ok().json(bi.publish(actor,id,body.expected_version).await?)),
            "execute" => {
                auth::require_permission(req,"bi:execute").await?;
                let result = bi.enqueue(actor,id,crate::services::bi_report::BiRunRequest {expected_version:body.expected_version,idempotency_key:key(&body.payload)?}).await?;
                let result = service::<ProductJobService>(req)?.submission_view(actor,result).await?;
                return Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/bi/personnel"})));
            },
            _ => {},
        }
    }
    if matches!(kind, "quality_object" | "quality_rule" | "quality_task") {
        let quality = service::<QualityService>(req)?;
        if action == "validate" {
            return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":quality.validate_saved(actor,kind,id,body.expected_version).await?})));
        }
        if kind == "quality_task" && action == "execute" {
            auth::require_permission(req, "quality:execute").await?;
            let result = quality
                .enqueue(
                    actor,
                    id,
                    QualityRunRequest {
                        expected_version: body.expected_version,
                        idempotency_key: key(&body.payload)?,
                    },
                )
                .await?;
            let result = service::<ProductJobService>(req)?
                .submission_view(actor, result)
                .await?;
            return Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/quality/results"})));
        }
    }
    if kind == "metadata_collection" {
        let metadata = service::<crate::services::metadata::MetadataService>(req)?;
        if action == "validate" {
            return Ok(HttpResponse::Ok().json(json!({"resource":resources.get(actor,kind,id).await?,"result":metadata.validate_saved(actor,id,body.expected_version).await?})));
        }
        if action == "execute" {
            let result = metadata
                .enqueue(
                    actor,
                    id,
                    crate::services::metadata::MetadataRunRequest {
                        expected_version: body.expected_version,
                        idempotency_key: key(&body.payload)?,
                    },
                )
                .await?;
            let result = service::<ProductJobService>(req)?
                .submission_view(actor, result)
                .await?;
            return Ok(HttpResponse::Accepted().json(json!({"resource":resources.get(actor,kind,id).await?,"result":{"job":result.job,"created":result.created},"run_id":result.job.id,"next_path":"/metadata/collection"})));
        }
    }
    if kind == "quality_monitor" && action == "evaluate" {
        let mut payload = body
            .payload
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("评估参数必须是对象".into()))?;
        if payload.contains_key("expected_version") {
            return Err(AppError::BadRequest("评估参数不能覆盖版本".into()));
        }
        payload.insert("expected_version".into(), json!(body.expected_version));
        let request = serde_json::from_value(json!(payload))
            .map_err(|_| AppError::BadRequest("评估参数仅支持稳定幂等键".into()))?;
        let result = service::<crate::services::quality_alerts::QualityAlertsService>(req)?
            .evaluate(actor, id, request)
            .await?;
        return Ok(HttpResponse::Ok()
            .json(json!({"resource":resources.get(actor,kind,id).await?,"result":result})));
    }
    Ok(HttpResponse::Ok().json(resources.action(actor, kind, id, action, body).await?))
}
