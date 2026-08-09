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

/// 创建并配置 actix-web 应用
pub async fn create_app(
    ctx: BackendContext,
    config: AppConfig,
) -> anyhow::Result<actix_web::dev::Server> {
    let bind_addr = format!("{}:{}", config.server_host, config.server_port);

    let auth_svc = AuthService::new(
        ctx.tenant_repo.clone(),
        ctx.user_repo.clone(),
        ctx.role_repo.clone(),
        config.jwt_secret.clone(),
        config.jwt_expiry_hours,
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

    let ws_hub = WsHub::new();

    // AI 服务
    let ai_svc = AiService::new(&config.deerflow);

    // SwanLake 查询服务
    let query_svc = QueryService::new(&config.swanlake);

    // 创建引擎实例（ape-dts）
    let engine: Arc<dyn dts_engine::EngineService> =
        Arc::new(dts_engine_ape::ApeEngineService::new());

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

    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_middleware::Logger::default())
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
            // 任务调度器
            .app_data(web::Data::new(scheduler.clone()))
            // jiascheduler 配置
            .app_data(web::Data::new(config.scheduler.clone()))
            // REST API v1
            .service(
                web::scope("/api/v1")
                    // 认证
                    .service(
                        web::scope("/auth")
                            .route("/login", web::post().to(crate::api::auth::login))
                            .route("/me", web::get().to(crate::api::auth::me))
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
                            .route("/{id}", web::get().to(crate::api::datasources::get))
                            .route("/{id}", web::delete().to(crate::api::datasources::delete)),
                    )
                    // 同步任务管理
                    .service(
                        web::scope("/tasks")
                            .route("", web::get().to(crate::api::tasks::list))
                            .route("", web::post().to(crate::api::tasks::create))
                            .route("/{id}", web::get().to(crate::api::tasks::get))
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
                            .route("/diagnose", web::post().to(crate::api::ai::diagnose)),
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

    Ok(server)
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
