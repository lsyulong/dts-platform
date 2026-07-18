//! 数据库基础设施 — 封装存储后端的初始化和装配
//!
//! 支持运行时后端分发：
//! - `sqlite` (默认 feature) — SQLite 后端，用于本地开发和测试
//! - `postgres` (可选 feature) — PostgreSQL 协议后端
//!
//! 通过 `AppConfig.database_backend` 在运行时选择后端。

use std::sync::Arc;

use dts_backend::pool::BackendPool;
use dts_backend::repo::{
    AlertRepo, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo, TaskRunRepo,
    TenantRepo, UserRepo,
};
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use dts_backend_rdbms::MigrationRunner;

/// 后端上下文 — 持有连接池和所有 Repository
///
/// `pool` 使用 `Arc<dyn BackendPool>`，不依赖具体后端类型，
/// 确保上层代码与存储后端解耦。
pub struct BackendContext {
    pub pool: Arc<dyn BackendPool>,
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub datasource_repo: Arc<dyn DatasourceRepo>,
    pub task_repo: Arc<dyn TaskRepo>,
    pub task_run_repo: Arc<dyn TaskRunRepo>,
    pub alert_repo: Arc<dyn AlertRepo>,
    pub monitoring_repo: Arc<dyn MonitoringRepo>,
    pub seed_repo: Arc<dyn SeedRepo>,
}

/// 根据配置的后端类型初始化存储后端
///
/// 在运行时根据 `backend_type` 分发到对应的初始化函数。
/// 可用的后端由编译时的 feature flags 决定。
#[allow(unused_variables)]
pub async fn init_backend(
    database_url: &str,
    backend_type: &str,
) -> anyhow::Result<BackendContext> {
    match backend_type {
        #[cfg(feature = "sqlite")]
        "sqlite" => init_sqlite(database_url).await,

        #[cfg(feature = "postgres")]
        "postgres" => init_postgres(database_url).await,

        other => {
            let available: &[&str] = &[
                #[cfg(feature = "sqlite")]
                "sqlite",
                #[cfg(feature = "postgres")]
                "postgres",
            ];

            let available_str = if available.is_empty() {
                "(无 — 请启用至少一个 feature flag)".to_string()
            } else {
                available.join(", ")
            };

            anyhow::bail!("不支持的数据后端: '{other}'。可用后端: {available_str}")
        },
    }
}

// ============================================================================
// SQLite 后端初始化
// ============================================================================

#[cfg(feature = "sqlite")]
async fn init_sqlite(database_url: &str) -> anyhow::Result<BackendContext> {
    use dts_backend_sqlite::{
        SqliteAlertRepo, SqliteDatasourceRepo, SqliteMigrationRunner, SqliteMonitoringRepo,
        SqliteRoleRepo, SqliteSeedRepo, SqliteTaskRepo, SqliteTaskRunRepo, SqliteTenantRepo,
        SqliteUserRepo,
    };

    // 1. 创建连接池
    let pool = dts_backend_sqlite::create_pool(database_url).await?;

    // 2. 运行迁移
    let inner_pool = pool.inner().clone();
    let migration_runner = SqliteMigrationRunner::new(inner_pool.clone());
    migration_runner.run_migrations().await?;

    // 3. 种子数据
    let seed_repo = Arc::new(SqliteSeedRepo::new(inner_pool.clone()));
    if !seed_repo.has_data().await? {
        seed_repo.seed().await?;
    } else {
        tracing::info!("数据库已有数据，跳过种子数据初始化");
    }

    // 4. 装配 Repository
    let ctx = BackendContext {
        pool: Arc::new(pool),
        tenant_repo: Arc::new(SqliteTenantRepo::new(inner_pool.clone())),
        user_repo: Arc::new(SqliteUserRepo::new(inner_pool.clone())),
        role_repo: Arc::new(SqliteRoleRepo::new(inner_pool.clone())),
        datasource_repo: Arc::new(SqliteDatasourceRepo::new(inner_pool.clone())),
        task_repo: Arc::new(SqliteTaskRepo::new(inner_pool.clone())),
        task_run_repo: Arc::new(SqliteTaskRunRepo::new(inner_pool.clone())),
        alert_repo: Arc::new(SqliteAlertRepo::new(inner_pool.clone())),
        monitoring_repo: Arc::new(SqliteMonitoringRepo::new(inner_pool.clone())),
        seed_repo,
    };

    tracing::info!("SQLite 后端初始化完成");
    Ok(ctx)
}

// ============================================================================
// PostgreSQL 后端初始化
// ============================================================================

#[cfg(feature = "postgres")]
async fn init_postgres(database_url: &str) -> anyhow::Result<BackendContext> {
    use dts_backend_postgres::{
        PgAlertRepo, PgDatasourceRepo, PgMigrationRunner, PgMonitoringRepo, PgRoleRepo, PgSeedRepo,
        PgTaskRepo, PgTaskRunRepo, PgTenantRepo, PgUserRepo,
    };

    // 1. 创建连接池
    let pool = dts_backend_postgres::create_pool(database_url).await?;

    // 2. 运行迁移
    let inner_pool = pool.inner().clone();
    let migration_runner = PgMigrationRunner::new(inner_pool.clone());
    migration_runner.run_migrations().await?;

    // 3. 种子数据
    let seed_repo = Arc::new(PgSeedRepo::new(inner_pool.clone()));
    if !seed_repo.has_data().await? {
        seed_repo.seed().await?;
    } else {
        tracing::info!("数据库已有数据，跳过种子数据初始化");
    }

    // 4. 装配 Repository
    let ctx = BackendContext {
        pool: Arc::new(pool),
        tenant_repo: Arc::new(PgTenantRepo::new(inner_pool.clone())),
        user_repo: Arc::new(PgUserRepo::new(inner_pool.clone())),
        role_repo: Arc::new(PgRoleRepo::new(inner_pool.clone())),
        datasource_repo: Arc::new(PgDatasourceRepo::new(inner_pool.clone())),
        task_repo: Arc::new(PgTaskRepo::new(inner_pool.clone())),
        task_run_repo: Arc::new(PgTaskRunRepo::new(inner_pool.clone())),
        alert_repo: Arc::new(PgAlertRepo::new(inner_pool.clone())),
        monitoring_repo: Arc::new(PgMonitoringRepo::new(inner_pool.clone())),
        seed_repo,
    };

    tracing::info!("PostgreSQL 后端初始化完成");
    Ok(ctx)
}
