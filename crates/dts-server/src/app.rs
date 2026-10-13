// Copyright 2026 DTS Platform Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, middleware as actix_middleware, web};

use crate::config::AppConfig;
use crate::db::BackendContext;
use crate::services::ai_svc::AiService;
use crate::services::alert_svc::AlertService;
use crate::services::auth_svc::AuthService;
use crate::services::datasource_svc::DatasourceService;
use crate::services::monitoring_svc::MonitoringService;
use crate::services::query_svc::QueryService;
use crate::services::scheduler::TaskScheduler;
use crate::services::task_svc::TaskService;
use crate::services::tenant_svc::TenantService;
use crate::services::user_svc::UserService;
use crate::ws::task_log::WsHub;

pub struct RunningApp {
    server: actix_web::dev::Server,
    worker: Option<crate::services::product_jobs::ProductJobWorkerHandle>,
    alert_worker: Option<crate::services::alert_automation::AlertAutomationWorkerHandle>,
}

impl RunningApp {
    pub async fn run(self) -> anyhow::Result<()> {
        let server_result = self.server.await;
        if let Some(worker) = self.worker {
            worker.shutdown().await?;
        }
        if let Some(worker) = self.alert_worker {
            worker.shutdown().await?;
        }
        Ok(server_result?)
    }
}

/// 创建并配置 actix-web 应用
pub async fn create_app(mut ctx: BackendContext, config: AppConfig) -> anyhow::Result<RunningApp> {
    let bind_addr = format!("{}:{}", config.server_host, config.server_port);
    ctx.datasource_repo =
        crate::services::credentials::protect_repo(ctx.datasource_repo, &config.data_dir)?;
    ctx.notification_connection_repo = crate::services::credentials::protect_notification_repo(
        ctx.notification_connection_repo,
        &config.data_dir,
    )?;

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        crate::services::sa_auth::build_manager(&config.auth, ctx.role_repo.clone()).await?,
    );

    let tenant_svc = TenantService::new(
        ctx.tenant_repo.clone(),
        ctx.role_repo.clone(),
        ctx.user_repo.clone(),
    );

    let task_svc = TaskService::new(ctx.task_repo.clone(), ctx.datasource_repo.clone());
    let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());
    let alert_svc = AlertService::new(ctx.alert_repo.clone());
    let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());
    let user_svc = UserService::new(ctx.user_repo.clone());
    let message_svc = crate::services::message_delivery::MessageService::new(
        ctx.product_resource_repo.clone(),
        ctx.user_repo.clone(),
        ctx.message_delivery_repo.clone(),
    );
    let content_svc = crate::services::system_content::SystemContentService::new(
        ctx.product_resource_repo.clone(),
        ctx.role_repo.clone(),
    )
    .with_organizations(ctx.organization_members_repo.clone());
    let master_svc = crate::services::master_data::MasterDataService::new(
        ctx.master_data_repo.clone(),
        ctx.product_resource_repo.clone(),
    );

    let task_log_svc =
        crate::services::task_log_svc::TaskLogService::new(ctx.task_log_repo.clone());
    let ws_hub = WsHub::with_log_service(task_log_svc.clone());

    // AI 服务
    let ai_svc = AiService::new(&config.deerflow);

    // SwanLake 查询服务
    let query_svc = QueryService::new(&config.swanlake);
    let query_access = crate::api::query::QueryAccess {
        tenant_id: if config.swanlake.enabled {
            Some(
                ctx.tenant_repo
                    .find_by_slug(&config.swanlake.tenant_slug)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("SWANLAKE_TENANT_SLUG 对应的租户不存在"))?
                    .id,
            )
        } else {
            None
        },
    };
    let standards_svc = crate::services::standards::StandardGovernanceService::new(
        ctx.standard_governance_repo.clone(),
        ctx.product_resource_repo.clone(),
        auth_svc.clone(),
    );
    let development_revisions_svc =
        crate::services::development_revisions::DevelopmentRevisionsService::new(
            ctx.development_revision_repo.clone(),
        );
    let runtime_configuration_svc =
        crate::services::runtime_configuration::RuntimeConfigurationService::new(
            ctx.runtime_configuration_repo.clone(),
            ctx.product_resource_repo.clone(),
        );
    let product_svc = crate::services::product::ProductService::new(
        ctx.product_resource_repo.clone(),
        query_svc.clone(),
        query_access.tenant_id.clone(),
    )
    .with_organizations(ctx.organization_members_repo.clone())
    .with_standards(Arc::new(standards_svc.clone()));
    let query_authorization = Arc::new(
        crate::services::data_access::QueryAuthorizationService::new(
            ctx.data_access_repo.clone(),
            ctx.metadata_catalog_repo.clone(),
            query_access.clone(),
        )
        .with_job_repo(ctx.product_job_repo.clone())
        .with_standard_repo(ctx.standard_governance_repo.clone())
        .with_resource_repo(ctx.product_resource_repo.clone()),
    );
    let data_access_svc = crate::services::data_access::DataAccessService::new(
        ctx.data_access_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        query_access.clone(),
    );
    let masking_svc = Arc::new(crate::services::data_masking::MaskingService::new(
        ctx.product_resource_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.user_repo.clone(),
        crate::services::data_masking::PrivacyKey::load_or_create(&config.data_dir)?,
    ));
    let job_svc = crate::services::product_jobs::ProductJobService::new(
        ctx.product_job_repo.clone(),
        ctx.product_resource_repo.clone(),
        auth_svc.clone(),
        query_svc.clone(),
        query_access.clone(),
    )
    .with_metadata_repo(ctx.metadata_catalog_repo.clone())
    .with_standard_repo(ctx.standard_governance_repo.clone())
    .with_runtime_configuration(ctx.runtime_configuration_repo.clone())
    .with_sensitive_findings_repo(ctx.sensitive_finding_repo.clone())
    .with_query_authorization(query_authorization.clone())
    .with_masking(masking_svc.clone());
    let development_schedule_svc =
        crate::services::development_schedules::DevelopmentScheduleService::new(
            ctx.product_resource_repo.clone(),
            auth_svc.clone(),
            config.scheduler.clone(),
        );
    let tag_assignment_svc = crate::services::tag_assignments::TagAssignmentService::new(
        ctx.tag_assignment_repo.clone(),
        ctx.product_resource_repo.clone(),
        auth_svc.clone(),
    );
    let alert_policy_svc = crate::services::alert_policies::AlertPolicyService::new(
        ctx.alert_policy_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.user_repo.clone(),
        auth_svc.clone(),
    );
    let notification_connection_svc =
        crate::services::notification_connections::NotificationConnectionService::new(
            ctx.notification_connection_repo.clone(),
        );
    let alert_automation_svc = crate::services::alert_automation::AlertAutomationService::new(
        ctx.alert_policy_repo.clone(),
        alert_policy_svc.clone(),
        config.alert_automation.clone(),
    )
    .with_notification_connections(notification_connection_svc.clone());
    let quality_svc = crate::services::data_quality::QualityService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        query_access.clone(),
    )
    .with_metadata_repo(ctx.metadata_catalog_repo.clone())
    .with_query_authorization(query_authorization.clone());
    let metrics_svc = crate::services::metrics::MetricsService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        auth_svc.clone(),
        query_access.clone(),
    )
    .with_query_authorization(query_authorization.clone())
    .with_standard_repo(ctx.standard_governance_repo.clone());
    let sensitive_svc = crate::services::sensitive_recognition::SensitiveRecognitionService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.sensitive_finding_repo.clone(),
        auth_svc.clone(),
        query_access.clone(),
    )
    .with_query_authorization(query_authorization.clone());
    let navigation_svc = crate::services::navigation::NavigationService::new(
        ctx.navigation_repo.clone(),
        crate::services::navigation::product_registry()?,
    );
    let organization_svc = crate::services::organization_members::OrganizationMembersService::new(
        ctx.organization_members_repo.clone(),
    );
    let projection_svc = crate::services::platform_projection::PlatformProjectionService::new(
        ctx.platform_projection_repo.clone(),
        auth_svc.clone(),
    );
    let quality_alert_svc = crate::services::quality_alerts::QualityAlertsService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        query_authorization.clone(),
        auth_svc.clone(),
        ctx.quality_alert_repo.clone(),
    );
    let metadata_svc = crate::services::metadata::MetadataService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        auth_svc.clone(),
        query_access.clone(),
    );
    let bi_svc = crate::services::bi_report::BiReportService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        auth_svc.clone(),
        query_access.clone(),
    );
    let lineage_svc = crate::services::metadata_lineage::MetadataLineageService::new(
        ctx.product_resource_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        ctx.metadata_lineage_repo.clone(),
    );
    let assets_svc = crate::services::assets::AssetsService::new(
        ctx.assets_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.metadata_lineage_repo.clone(),
    );
    let data_api_svc = crate::services::data_services::DataApiService::new(
        ctx.product_resource_repo.clone(),
        ctx.product_job_repo.clone(),
        ctx.metadata_catalog_repo.clone(),
        auth_svc.clone(),
        query_access.clone(),
    );
    let service_access_svc = crate::services::service_access::ServiceAccessService::new(
        ctx.service_access_repo.clone(),
        ctx.product_resource_repo.clone(),
        ctx.user_repo.clone(),
        crate::services::service_access::CredentialKey::load_or_create(&config.data_dir)?,
    );
    let service_invocation_svc = crate::services::service_invocation::ServiceInvocationService::new(
        service_access_svc.clone(),
        data_api_svc.clone(),
        Arc::new(query_svc.clone()),
        query_authorization.clone(),
        masking_svc.clone(),
    );
    let result_exports = Arc::new(crate::services::product_exports::ResultExportService::new(
        Arc::new(job_svc.clone()),
    ));
    let bi_share_svc = crate::services::bi_share::BiShareService::new(
        ctx.bi_share_repo.clone(),
        Arc::new(job_svc.clone()),
        result_exports.clone(),
        auth_svc.clone(),
    );

    // Engine-specific routing remains behind one scheduler interface.
    let engine: Arc<dyn dts_engine::EngineService> =
        Arc::new(dts_engine::EngineRouter::new(vec![
            Arc::new(dts_engine_swanlake::SwanLakeEngineService::new()),
            Arc::new(dts_engine_ape::ApeEngineService::new()),
        ])?);

    // 创建任务调度器（连接 engine + repos + ws）
    let scheduler = TaskScheduler::new(
        engine,
        ctx.task_repo.clone(),
        ctx.task_run_repo.clone(),
        ctx.datasource_repo.clone(),
        Arc::new(ws_hub.clone()),
    );

    // 验证 backend 连接
    ctx.pool.ping().await?;
    let worker_service = job_svc.clone();
    let alert_worker_service = alert_automation_svc.clone();

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_middleware::Logger::new("%a %m %U %s %b %T"))
            // 后端连接池 (用于健康检查等)
            .app_data(web::Data::new(ctx.pool.clone()))
            // 认证服务
            .app_data(web::Data::new(auth_svc.clone()))
            // 租户服务
            .app_data(web::Data::new(tenant_svc.clone()))
            // 业务服务
            .app_data(web::Data::new(task_svc.clone()))
            .app_data(web::Data::new(datasource_svc.clone()))
            .app_data(web::Data::new(alert_svc.clone()))
            .app_data(web::Data::new(monitoring_svc.clone()))
            .app_data(web::Data::new(user_svc.clone()))
            .app_data(web::Data::new(message_svc.clone()))
            .app_data(web::Data::new(content_svc.clone()))
            .app_data(web::Data::new(master_svc.clone()))
            .app_data(web::Data::new(job_svc.clone()))
            .app_data(web::Data::new(development_schedule_svc.clone()))
            .app_data(web::Data::new(tag_assignment_svc.clone()))
            .app_data(web::Data::new(alert_policy_svc.clone()))
            .app_data(web::Data::new(notification_connection_svc.clone()))
            .app_data(web::Data::new(quality_svc.clone()))
            .app_data(web::Data::new(metrics_svc.clone()))
            .app_data(web::Data::new(sensitive_svc.clone()))
            .app_data(web::Data::new(navigation_svc.clone()))
            .app_data(web::Data::new(organization_svc.clone()))
            .app_data(web::Data::new(projection_svc.clone()))
            .app_data(web::Data::new(quality_alert_svc.clone()))
            .app_data(web::Data::new(standards_svc.clone()))
            .app_data(web::Data::new(development_revisions_svc.clone()))
            .app_data(web::Data::new(runtime_configuration_svc.clone()))
            .app_data(web::Data::new(metadata_svc.clone()))
            .app_data(web::Data::new(bi_svc.clone()))
            .app_data(web::Data::new(lineage_svc.clone()))
            .app_data(web::Data::new(assets_svc.clone()))
            .app_data(web::Data::new(data_api_svc.clone()))
            .app_data(web::Data::new(service_access_svc.clone()))
            .app_data(web::Data::new(service_invocation_svc.clone()))
            .app_data(web::Data::from(result_exports.clone()))
            .app_data(web::Data::new(bi_share_svc.clone()))
            .app_data(web::Data::new(data_access_svc.clone()))
            .app_data(web::Data::from(query_authorization.clone()))
            .app_data(web::Data::from(masking_svc.clone()))
            .app_data(web::Data::new(task_log_svc.clone()))
            .app_data(web::Data::new(product_svc.clone()))
            // Repository traits (仍被部分 handler 直接使用)
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .app_data(web::Data::new(ctx.datasource_repo.clone()))
            .app_data(web::Data::new(ctx.task_repo.clone()))
            .app_data(web::Data::new(ctx.task_run_repo.clone()))
            // WebSocket Hub
            .app_data(web::Data::new(ws_hub.clone()))
            // AI 服务
            .app_data(web::Data::new(ai_svc.clone()))
            // SwanLake 查询服务
            .app_data(web::Data::new(query_svc.clone()))
            .app_data(web::Data::new(query_access.clone()))
            .app_data(web::Data::new(ctx.tenant_repo.clone()))
            // 任务调度器
            .app_data(web::Data::new(scheduler.clone()))
            // jiascheduler 配置
            .app_data(web::Data::new(config.scheduler.clone()))
            // REST API v1
            .service(
                web::scope("/api/v1")
                    .service(web::resource("/data-services/{api_id}/invoke")
                        .app_data(crate::api::service_invocation::json_config())
                        .route(web::post().to(crate::api::service_invocation::invoke)))
                    .service(web::scope("/product")
                        .route("/standards/{kind}/{id}/revisions",web::get().to(crate::api::standards::revisions))
                        .route("/standards/consumers/data_model/{id}/bindings",web::get().to(crate::api::standards::bindings))
                        .route("/standards/consumers/data_model/{id}/bindings",web::post().to(crate::api::standards::bind_model))
                        .route("/standards/consumers/data_model/{id}/inspect",web::post().to(crate::api::standards::inspect))
                        .route("/development/{kind}/{id}/revisions",web::get().to(crate::api::development_revisions::history))
                        .route("/development/{kind}/{id}/revisions",web::post().to(crate::api::development_revisions::save))
                        .route("/development/{kind}/{id}/revisions/{revision_id}",web::get().to(crate::api::development_revisions::detail))
                        .route("/development/{kind}/{id}/revisions/{revision_id}/restore",web::post().to(crate::api::development_revisions::restore))
                        .route("/development/{kind}/{id}/diff",web::get().to(crate::api::development_revisions::diff))
                        .route("/resources/development_workflow/{id}/schedule",web::get().to(crate::api::development_schedules::status))
                        .route("/resources/tag/{id}/assignments",web::get().to(crate::api::tag_assignments::list))
                        .route("/resources/alert_policy/{id}/evaluations",web::get().to(crate::api::alert_policies::evaluations))
                        .route("/work-items",web::get().to(crate::api::platform_projection::work_items))
                        .route("/statistics",web::get().to(crate::api::platform_projection::statistics))
                        .route("/quality-monitors/{id}/evaluate",web::post().to(crate::api::quality_alerts::evaluate))
                        .route("/quality-monitors/{id}/observations",web::get().to(crate::api::quality_alerts::observations))
                        .route("/quality-monitors/{id}/alerts",web::get().to(crate::api::quality_alerts::alerts))
                        .route("/quality-alerts/{id}/acknowledge",web::post().to(crate::api::quality_alerts::acknowledge))
                        .route("/quality-alerts/{id}/events",web::get().to(crate::api::quality_alerts::events))
                        .route("/quality-notices",web::get().to(crate::api::quality_alerts::notices))
                        .route("/quality-notices/{id}/read",web::post().to(crate::api::quality_alerts::read_notice))
                        .route("/organizations/{id}",web::get().to(crate::api::organization_members::organization))
                        .route("/organizations/{id}/members",web::get().to(crate::api::organization_members::members))
                        .route("/organizations/{id}/manager",web::post().to(crate::api::organization_members::manager))
                        .route("/organization-members/history",web::get().to(crate::api::organization_members::history))
                        .route("/organization-members/users/{user_id}",web::get().to(crate::api::organization_members::membership))
                        .route("/organization-members/users/{user_id}/transfer",web::post().to(crate::api::organization_members::transfer))
                        .route("/organization-members/users/{user_id}/remove",web::post().to(crate::api::organization_members::remove))
                        .route("/navigation/current",web::get().to(crate::api::navigation::current))
                        .route("/navigation/history",web::get().to(crate::api::navigation::history))
                        .route("/navigation/assign/{template_id}",web::post().to(crate::api::navigation::assign))
                        .route("/navigation/revoke",web::post().to(crate::api::navigation::revoke))
                        .route("/sensitive-findings",web::get().to(crate::api::sensitive_recognition::list))
                        .route("/sensitive-findings/{id}",web::get().to(crate::api::sensitive_recognition::get))
                        .route("/sensitive-findings/{id}/{action}",web::post().to(crate::api::sensitive_recognition::review))
                        .route("/reference/users", web::get().to(crate::api::product_references::users))
                        .route("/assets/usage", web::post().to(crate::api::assets::usage))
                        .route("/bi-shares",web::get().to(crate::api::bi_share::list))
                        .route("/bi-shares/{id}",web::get().to(crate::api::bi_share::get))
                        .route("/bi-shares/{id}/open",web::get().to(crate::api::bi_share::open))
                        .route("/bi-shares/{id}/revoke",web::post().to(crate::api::bi_share::revoke))
                        .route("/bi-shares/{id}/export",web::post().to(crate::api::bi_share::export))
                        .route("/resources/bi_report/{id}/shares",web::post().to(crate::api::bi_share::create))
                        .route("/service-access/approvals",web::get().to(crate::api::service_access::approvals))
                        .route("/service-access/calls",web::get().to(crate::api::service_access::calls))
                        .route("/service-access/applications/{id}/credentials",web::get().to(crate::api::service_access::credentials))
                        .route("/service-access/applications/{id}/credentials",web::post().to(crate::api::service_access::issue))
                        .route("/service-access/applications/{application_id}/credentials/{credential_id}/revoke",web::post().to(crate::api::service_access::revoke))
                        .route("/data-access/approvals", web::get().to(crate::api::data_access::approvals))
                        .route("/data-access/audit", web::get().to(crate::api::data_access::audit))
                        .route("/data-access/grants", web::get().to(crate::api::data_access::grants))
                        .route("/data-access/requests/{id}/actions/{action}", web::post().to(crate::api::data_access::decision))
                        .route("/content/{kind}", web::get().to(crate::api::system_content::visible))
                        .route("/quality/results", web::get().to(crate::api::data_quality::results))
                        .route("/quality/report", web::post().to(crate::api::data_quality::report))
                        .route("/jobs", web::get().to(crate::api::product_jobs::list))
                        .route("/jobs/{id}", web::get().to(crate::api::product_jobs::get))
                        .route("/jobs/{id}/cancel", web::post().to(crate::api::product_jobs::cancel))
                        .route("/jobs/{id}/retry", web::post().to(crate::api::product_jobs::retry))
                        .route("/jobs/{id}/export",web::post().to(crate::api::product_exports::export))
                        .route("/{kind}/{id}/jobs", web::post().to(crate::api::product_jobs::enqueue))
                        .route("/messages/inbox", web::get().to(crate::api::messages::inbox))
                        .route("/messages/{id}/read", web::post().to(crate::api::messages::mark_read))
                        .route("/schemas", web::get().to(crate::api::product::schemas))
                        .route("/resources/{kind}", web::get().to(crate::api::product::list))
                        .route("/resources/{kind}", web::post().to(crate::api::product::create))
                        .route("/resources/{kind}/{id}", web::get().to(crate::api::product::get))
                        .route("/resources/{kind}/{id}", web::put().to(crate::api::product::update))
                        .route("/resources/{kind}/{id}", web::delete().to(crate::api::product::delete))
                        .route("/resources/{kind}/{id}/audit", web::get().to(crate::api::product::audit))
                        .route("/resources/{kind}/{id}/actions/{action}", web::post().to(crate::api::product::action)))
                    .route("/internal/tasks/{id}/start", web::post().to(crate::api::scheduler_trigger::start))
                    .route("/internal/development/workflows/{id}/trigger", web::post().to(crate::api::scheduler_trigger::start_workflow))
                    .service(
                        web::scope("/notification-connections")
                            .route("", web::get().to(crate::api::notification_connections::list))
                            .route("", web::post().to(crate::api::notification_connections::create))
                            .route("/{id}", web::get().to(crate::api::notification_connections::get))
                            .route("/{id}", web::put().to(crate::api::notification_connections::update))
                            .route("/{id}", web::delete().to(crate::api::notification_connections::delete))
                            .route("/{id}/enabled", web::patch().to(crate::api::notification_connections::enabled))
                            .route("/{id}/test", web::post().to(crate::api::notification_connections::test)),
                    )
                    // 认证
                    .service(
                        web::scope("/auth")
                            .route("/login", web::post().to(crate::api::auth::login))
                            .route("/logout", web::post().to(crate::api::auth::logout))
                            .route("/me", web::get().to(crate::api::auth::me))
                            .route("/profile", web::patch().to(crate::api::auth::update_profile))
                            .route("/change-password", web::post().to(crate::api::auth::change_password))
                            .route("/permissions", web::get().to(crate::api::auth::my_permissions)),
                    )
                    // 租户管理
                    .service(
                        web::scope("/tenants")
                            .route("", web::post().to(crate::api::tenants::create))
                            .route("", web::get().to(crate::api::tenants::list)),
                    )
                    // 数据源管理
                    .service(
                        web::scope("/datasources")
                            .route("", web::get().to(crate::api::datasources::list))
                            .route("", web::post().to(crate::api::datasources::create))
                            .route("/{id}/test", web::post().to(crate::api::datasources::test_connection))
                            .route("/{id}/probes", web::get().to(crate::api::datasources::probe_history))
                            .route("/{id}/tables", web::get().to(crate::api::datasources::tables))
                            .route("/{id}", web::get().to(crate::api::datasources::get))
                            .route("/{id}", web::delete().to(crate::api::datasources::delete)),
                    )
                    // 同步任务管理
                    .service(
                        web::scope("/tasks")
                            .route("", web::get().to(crate::api::tasks::list))
                            .route("", web::post().to(crate::api::tasks::create))
                            .route("/{id}", web::get().to(crate::api::tasks::get))
                            .route("/{task_id}/runs/{run_id}/logs", web::get().to(crate::api::task_logs::list))
                            .route("/{id}/start", web::post().to(crate::api::tasks::start))
                            .route("/{id}/schedule", web::post().to(crate::api::scheduler::set_schedule))
                            .route("/{id}/schedule", web::get().to(crate::api::scheduler::get_schedule))
                            .route("/{id}/schedule", web::delete().to(crate::api::scheduler::delete_schedule))
                            .route("/{id}/status", web::patch().to(crate::api::tasks::update_status)),
                    )
                    // 监控
                    .service(
                        web::scope("/monitoring")
                            .route("/overview", web::get().to(crate::api::monitoring::overview)),
                    )
                    // 告警
                    .service(
                        web::scope("/alerts")
                            .route("", web::get().to(crate::api::alerts::list))
                            .route("", web::post().to(crate::api::alerts::create))
                            .route("/{id}/toggle", web::patch().to(crate::api::alerts::toggle)),
                    )
                    // AI 助手
                    .service(
                        web::scope("/ai")
                            .route("/chat", web::post().to(crate::api::ai::chat))
                            .route("/diagnose", web::post().to(crate::api::ai::diagnose))
                            .route("/report", web::post().to(crate::api::ai::report)),
                    )
                    // 查询分析 (SwanLake)
                    .service(
                        web::scope("/query")
                            .route("/execute", web::post().to(crate::api::query::execute))
                            .route("/schemas", web::get().to(crate::api::query::schemas)),
                    )
                    // 系统设置
                    .service(
                        web::scope("/system")
                            // 用户管理
                            .route("/users", web::get().to(crate::api::system::list_users))
                            .route("/users", web::post().to(crate::api::system::create_user))
                            .route("/users/{id}", web::put().to(crate::api::system::update_user))
                            .route("/users/{id}/reset-password", web::post().to(crate::api::system::reset_password))
                            .route("/users/{id}", web::delete().to(crate::api::system::delete_user))
                            // 角色管理
                            .route("/roles", web::get().to(crate::api::system::list_roles))
                            .route("/roles", web::post().to(crate::api::system::create_role))
                            .route("/roles/{id}", web::put().to(crate::api::system::update_role))
                            .route("/roles/{id}", web::delete().to(crate::api::system::delete_role))
                            // 租户管理
                            .route("/tenants", web::get().to(crate::api::system::list_tenants))
                            .route("/tenants/{id}", web::put().to(crate::api::system::update_tenant))
                            // 权限列表
                            .route("/permissions", web::get().to(crate::api::system::list_permissions)),
                    ),
            )
            // WebSocket
            .route("/ws/tasks/{task_id}", web::get().to(crate::ws::task_log::task_log_ws))
            // 健康检查
            .route("/health", web::get().to(health_check))
    })
    .bind(&bind_addr)?
    .run();

    Ok(RunningApp {
        server,
        worker: worker_service.spawn_worker(),
        alert_worker: alert_worker_service.spawn_worker(),
    })
}

/// 健康检查端点
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "dts-platform",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use pretty_assertions::assert_eq;

    // ========== health_check Tests ==========

    #[tokio::test]
    async fn health_check_returns_200() {
        let resp = health_check().await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_check_returns_correct_json_structure() {
        let resp = health_check().await;
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("application/json"));
    }

    #[actix_web::test]
    async fn health_check_body_contains_expected_fields() {
        let resp = health_check().await;
        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "healthy");
        assert_eq!(json["service"], "dts-platform");
        assert!(json["version"].is_string());
        assert!(!json["version"].as_str().unwrap().is_empty());
    }

    #[test]
    fn health_check_version_is_not_empty() {
        let expected_version = env!("CARGO_PKG_VERSION");
        assert!(!expected_version.is_empty());
    }
}
