use dts_server::config::AppConfig;
use dts_server::{app, db};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info,dts_server=debug"));

    log::info!("=== DTS Platform 企业级数据同步管理平台 ===");
    log::info!("版本: {}", env!("CARGO_PKG_VERSION"));

    // 加载配置
    let config = AppConfig::from_env()?;
    log::info!("服务地址: {}:{}", config.server_host, config.server_port);

    // 创建数据库连接池
    let pool = db::create_pool(&config.database_url).await?;

    // 运行数据库迁移
    db::run_migrations(&pool).await?;

    // 初始化种子数据（仅在空数据库时）
    db::seed_default_data(&pool).await?;

    // 启动 Web 服务
    let server = app::create_app(pool, config.clone()).await?;
    log::info!("服务已启动!");
    log::info!("健康检查: http://localhost:{}/health", config.server_port);
    log::info!("API 文档: 见 docs/api/ 目录");
    log::info!("默认管理员: admin / admin123 (租户: default)");

    server.await?;

    Ok(())
}
