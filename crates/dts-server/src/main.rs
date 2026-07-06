use dts_server::config::AppConfig;
use dts_server::{app, db};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::init_from_env(
        env_logger::Env::default().default_filter_or("info,dts_server=debug"),
    );

    log::info!("=== DTS Platform 企业级数据同步管理平台 ===");
    log::info!("版本: {}", env!("CARGO_PKG_VERSION"));

    // 加载配置
    let config = AppConfig::from_env()?;
    log::info!("服务地址: {}:{}", config.server_host, config.server_port);
    log::info!("存储后端: {}", config.database_backend);

    // 初始化存储后端
    let ctx = db::init_postgres(&config.database_url).await?;

    // 启动 Web 服务
    let server = app::create_app(ctx, config.clone()).await?;
    log::info!("服务已启动!");
    log::info!("健康检查: http://localhost:{}/health", config.server_port);
    log::info!("API 文档: 见 docs/api/ 目录");
    log::info!("默认管理员: admin / admin123 (租户: default)");

    server.await?;

    Ok(())
}
