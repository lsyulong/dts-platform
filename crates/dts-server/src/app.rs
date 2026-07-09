use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, middleware as actix_middleware, web};

use crate::config::AppConfig;
use crate::db::BackendContext;
use crate::services::auth_svc::AuthService;
use crate::services::tenant_svc::TenantService;
use crate::ws::task_log::WsHub;

/// 创建并配置 actix-web 应用
pub async fn create_app(ctx: BackendContext, config: AppConfig) -> anyhow::Result<actix_web::dev::Server> {
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

    let ws_hub = WsHub::new();

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
            // Repository traits
            .app_data(web::Data::new(ctx.user_repo.clone()))
            .app_data(web::Data::new(ctx.role_repo.clone()))
            .app_data(web::Data::new(ctx.datasource_repo.clone()))
            .app_data(web::Data::new(ctx.task_repo.clone()))
            .app_data(web::Data::new(ctx.alert_repo.clone()))
            .app_data(web::Data::new(ctx.monitoring_repo.clone()))
            // WebSocket Hub
            .app_data(web::Data::new(ws_hub.clone()))
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
