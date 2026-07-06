//! 数据库基础设施 — 封装存储后端的初始化和装配
//!
//! 当前使用 PostgreSQL 协议后端 (dts-backend-postgres)。
//! 切换后端只需修改本模块的 use 语句和 BackendContext 中的类型。

use std::sync::Arc;

use dts_backend::repo::{
    AlertRepo, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo, TenantRepo, UserRepo,
};
use dts_backend_rdbms::MigrationRunner;
use dts_backend_postgres::{
    PgAlertRepo, PgBackendPool, PgDatasourceRepo, PgMigrationRunner, PgMonitoringRepo, PgRoleRepo,
    PgSeedRepo, PgTaskRepo, PgTenantRepo, PgUserRepo,
};

/// 后端上下文 — 持有连接池和所有 Repository
pub struct BackendContext {
    pub pool: PgBackendPool,
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub datasource_repo: Arc<dyn DatasourceRepo>,
    pub task_repo: Arc<dyn TaskRepo>,
    pub alert_repo: Arc<dyn AlertRepo>,
    pub monitoring_repo: Arc<dyn MonitoringRepo>,
    pub seed_repo: Arc<dyn SeedRepo>,
}

/// 初始化 PostgreSQL 后端
pub async fn init_postgres(database_url: &str) -> anyhow::Result<BackendContext> {
    // 1. 创建连接池
    let pool = dts_backend_postgres::create_pool(database_url).await?;

    // 2. 运行迁移
    let pg_pool = pool.inner().clone();
    let migration_runner = PgMigrationRunner::new(pg_pool.clone());
    migration_runner.run_migrations().await?;

    // 3. 种子数据
    let seed_repo = Arc::new(PgSeedRepo::new(pg_pool.clone()));
    if !seed_repo.has_data().await? {
        seed_repo.seed().await?;
    } else {
        log::info!("数据库已有数据，跳过种子数据初始化");
    }

    // 4. 装配 Repository
    let ctx = BackendContext {
        pool: pool.clone(),
        tenant_repo: Arc::new(PgTenantRepo::new(pg_pool.clone())),
        user_repo: Arc::new(PgUserRepo::new(pg_pool.clone())),
        role_repo: Arc::new(PgRoleRepo::new(pg_pool.clone())),
        datasource_repo: Arc::new(PgDatasourceRepo::new(pg_pool.clone())),
        task_repo: Arc::new(PgTaskRepo::new(pg_pool.clone())),
        alert_repo: Arc::new(PgAlertRepo::new(pg_pool.clone())),
        monitoring_repo: Arc::new(PgMonitoringRepo::new(pg_pool.clone())),
        seed_repo,
    };

    log::info!("PostgreSQL 后端初始化完成");
    Ok(ctx)
}
