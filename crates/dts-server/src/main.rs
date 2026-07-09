use dts_server::config::AppConfig;
use dts_server::{app, db};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // 初始化 tracing 基础设施
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,dts_server=debug")),
        )
        .init();

    // 桥接 log crate 的宏到 tracing（其他模块仍使用 log）
    tracing_log::LogTracer::init().ok();

    tracing::info!("=== DTS Platform 企业级数据同步管理平台 ===");
    tracing::info!("版本: {}", env!("CARGO_PKG_VERSION"));

    // 加载配置
    let config = AppConfig::from_env()?;
    tracing::info!("服务地址: {}:{}", config.server_host, config.server_port);
    tracing::info!("存储后端: {}", config.database_backend);

    // 初始化存储后端（运行时分发）
    let ctx = db::init_backend(&config.database_url, &config.database_backend).await?;

    // 启动 Web 服务
    let server = app::create_app(ctx, config.clone()).await?;
    tracing::info!("服务已启动!");
    tracing::info!("健康检查: http://localhost:{}/health", config.server_port);
    tracing::info!("API 文档: 见 docs/api/ 目录");
    tracing::info!("默认管理员: admin / admin123 (租户: default)");

    server.await?;

    Ok(())
}
