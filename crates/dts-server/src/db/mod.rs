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
    AlertPolicyRepo, AlertRepo, AssetsRepo, BiShareRepo, DataAccessRepo, DatasourceRepo,
    DevelopmentRevisionRepo, MasterDataRepo, MessageDeliveryRepo, MetadataCatalogRepo,
    MetadataLineageRepo, MonitoringRepo, NavigationRepo, NotificationConnectionRepo,
    OrganizationMembersRepo, PlatformProjectionRepo, ProductJobRepo, ProductResourceRepo,
    QualityAlertRepo, RoleRepo, RuntimeConfigurationRepo, SeedRepo, SensitiveFindingRepo,
    ServiceAccessRepo, StandardGovernanceRepo, TagAssignmentRepo, TaskLogRepo, TaskRepo,
    TaskRunRepo, TenantRepo, UserRepo,
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
    pub task_log_repo: Arc<dyn TaskLogRepo>,
    pub alert_repo: Arc<dyn AlertRepo>,
    pub monitoring_repo: Arc<dyn MonitoringRepo>,
    pub seed_repo: Arc<dyn SeedRepo>,
    pub product_resource_repo: Arc<dyn ProductResourceRepo>,
    pub product_job_repo: Arc<dyn ProductJobRepo>,
    pub message_delivery_repo: Arc<dyn MessageDeliveryRepo>,
    pub master_data_repo: Arc<dyn MasterDataRepo>,
    pub metadata_catalog_repo: Arc<dyn MetadataCatalogRepo>,
    pub metadata_lineage_repo: Arc<dyn MetadataLineageRepo>,
    pub data_access_repo: Arc<dyn DataAccessRepo>,
    pub assets_repo: Arc<dyn AssetsRepo>,
    pub service_access_repo: Arc<dyn ServiceAccessRepo>,
    pub bi_share_repo: Arc<dyn BiShareRepo>,
    pub sensitive_finding_repo: Arc<dyn SensitiveFindingRepo>,
    pub navigation_repo: Arc<dyn NavigationRepo>,
    pub organization_members_repo: Arc<dyn OrganizationMembersRepo>,
    pub platform_projection_repo: Arc<dyn PlatformProjectionRepo>,
    pub quality_alert_repo: Arc<dyn QualityAlertRepo>,
    pub standard_governance_repo: Arc<dyn StandardGovernanceRepo>,
    pub development_revision_repo: Arc<dyn DevelopmentRevisionRepo>,
    pub runtime_configuration_repo: Arc<dyn RuntimeConfigurationRepo>,
    pub tag_assignment_repo: Arc<dyn TagAssignmentRepo>,
    pub alert_policy_repo: Arc<dyn AlertPolicyRepo>,
    pub notification_connection_repo: Arc<dyn NotificationConnectionRepo>,
}

/// Initialize development/test fixtures, including demonstration credentials.
/// Production startup must use `init_backend_with_bootstrap` instead.
pub async fn init_backend(
    database_url: &str,
    backend_type: &str,
) -> anyhow::Result<BackendContext> {
    init_backend_internal(database_url, backend_type, SeedMode::DevelopmentFixture).await
}

/// Initialize production storage. An empty database requires an explicit initial
/// administrator password; existing accounts are never reset on restart.
pub async fn init_backend_with_bootstrap(
    database_url: &str,
    backend_type: &str,
    admin_password: Option<&str>,
) -> anyhow::Result<BackendContext> {
    init_backend_internal(
        database_url,
        backend_type,
        SeedMode::Bootstrap(admin_password),
    )
    .await
}

#[derive(Clone, Copy)]
enum SeedMode<'a> {
    DevelopmentFixture,
    Bootstrap(Option<&'a str>),
    ExistingAccountsOnly,
}

/// Assemble repositories for an explicit offline command without seeding any accounts.
pub async fn init_backend_for_command(
    database_url: &str,
    backend_type: &str,
) -> anyhow::Result<BackendContext> {
    init_backend_internal(database_url, backend_type, SeedMode::ExistingAccountsOnly).await
}

#[allow(unused_variables)]
async fn init_backend_internal(
    database_url: &str,
    backend_type: &str,
    seed_mode: SeedMode<'_>,
) -> anyhow::Result<BackendContext> {
    match backend_type {
        #[cfg(feature = "sqlite")]
        "sqlite" => init_sqlite(database_url, seed_mode).await,

        #[cfg(feature = "postgres")]
        "postgres" => init_postgres(database_url, seed_mode).await,

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

#[cfg(any(feature = "sqlite", feature = "postgres"))]
async fn initialize_seed(repo: &dyn SeedRepo, mode: SeedMode<'_>) -> anyhow::Result<()> {
    if matches!(mode, SeedMode::ExistingAccountsOnly) {
        return Ok(());
    }
    if repo.has_data().await? {
        tracing::info!("数据库已有数据，跳过管理员初始化");
        return Ok(());
    }
    match mode {
        SeedMode::DevelopmentFixture => repo.seed().await?,
        SeedMode::Bootstrap(password) => {
            let password = password
                .filter(|password| password.len() >= 12)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "空数据库首次启动必须设置至少 12 字节的 BOOTSTRAP_ADMIN_PASSWORD"
                    )
                })?;
            repo.seed_bootstrap(password).await?;
        },
        SeedMode::ExistingAccountsOnly => {},
    }
    Ok(())
}

// ============================================================================
// SQLite 后端初始化
// ============================================================================

#[cfg(feature = "sqlite")]
async fn init_sqlite(
    database_url: &str,
    seed_mode: SeedMode<'_>,
) -> anyhow::Result<BackendContext> {
    use dts_backend_sqlite::{
        SqliteAlertPolicyRepo, SqliteAlertRepo, SqliteBiShareRepo, SqliteDataAccessRepo,
        SqliteDatasourceRepo, SqliteDevelopmentRevisionRepo, SqliteMasterDataRepo,
        SqliteMessageDeliveryRepo, SqliteMetadataCatalogRepo, SqliteMigrationRunner,
        SqliteMonitoringRepo, SqliteNavigationRepo, SqliteNotificationConnectionRepo,
        SqliteOrganizationMembersRepo, SqlitePlatformProjectionRepo, SqliteProductJobRepo,
        SqliteProductResourceRepo, SqliteQualityAlertRepo, SqliteRoleRepo,
        SqliteRuntimeConfigurationRepo, SqliteSeedRepo, SqliteSensitiveFindingRepo,
        SqliteServiceAccessRepo, SqliteStandardGovernanceRepo, SqliteTagAssignmentRepo,
        SqliteTaskLogRepo, SqliteTaskRepo, SqliteTaskRunRepo, SqliteTenantRepo, SqliteUserRepo,
    };

    // 1. 创建连接池
    let pool = dts_backend_sqlite::create_pool(database_url).await?;

    // 2. 运行迁移
    let inner_pool = pool.inner().clone();
    let migration_runner = SqliteMigrationRunner::new(inner_pool.clone());
    migration_runner.run_migrations().await?;

    // 3. 种子数据
    let seed_repo = Arc::new(SqliteSeedRepo::new(inner_pool.clone()));
    initialize_seed(seed_repo.as_ref(), seed_mode).await?;

    // 4. 装配 Repository
    let ctx = BackendContext {
        pool: Arc::new(pool),
        tenant_repo: Arc::new(SqliteTenantRepo::new(inner_pool.clone())),
        user_repo: Arc::new(SqliteUserRepo::new(inner_pool.clone())),
        role_repo: Arc::new(SqliteRoleRepo::new(inner_pool.clone())),
        datasource_repo: Arc::new(SqliteDatasourceRepo::new(inner_pool.clone())),
        task_repo: Arc::new(SqliteTaskRepo::new(inner_pool.clone())),
        task_run_repo: Arc::new(SqliteTaskRunRepo::new(inner_pool.clone())),
        task_log_repo: Arc::new(SqliteTaskLogRepo::new(inner_pool.clone())),
        alert_repo: Arc::new(SqliteAlertRepo::new(inner_pool.clone())),
        monitoring_repo: Arc::new(SqliteMonitoringRepo::new(inner_pool.clone())),
        product_resource_repo: Arc::new(SqliteProductResourceRepo::new(inner_pool.clone())),
        product_job_repo: Arc::new(SqliteProductJobRepo::new(inner_pool.clone())),
        message_delivery_repo: Arc::new(SqliteMessageDeliveryRepo::new(inner_pool.clone())),
        master_data_repo: Arc::new(SqliteMasterDataRepo::new(inner_pool.clone())),
        metadata_catalog_repo: Arc::new(SqliteMetadataCatalogRepo::new(inner_pool.clone())),
        metadata_lineage_repo: Arc::new(SqliteProductResourceRepo::new(inner_pool.clone())),
        data_access_repo: Arc::new(SqliteDataAccessRepo::new(inner_pool.clone())),
        assets_repo: Arc::new(SqliteProductResourceRepo::new(inner_pool.clone())),
        service_access_repo: Arc::new(SqliteServiceAccessRepo::new(inner_pool.clone())),
        bi_share_repo: Arc::new(SqliteBiShareRepo::new(inner_pool.clone())),
        sensitive_finding_repo: Arc::new(SqliteSensitiveFindingRepo::new(inner_pool.clone())),
        navigation_repo: Arc::new(SqliteNavigationRepo::new(inner_pool.clone())),
        organization_members_repo: Arc::new(SqliteOrganizationMembersRepo::new(inner_pool.clone())),
        platform_projection_repo: Arc::new(SqlitePlatformProjectionRepo::new(inner_pool.clone())),
        quality_alert_repo: Arc::new(SqliteQualityAlertRepo::new(inner_pool.clone())),
        standard_governance_repo: Arc::new(SqliteStandardGovernanceRepo::new(inner_pool.clone())),
        development_revision_repo: Arc::new(SqliteDevelopmentRevisionRepo::new(inner_pool.clone())),
        runtime_configuration_repo: Arc::new(SqliteRuntimeConfigurationRepo::new(
            inner_pool.clone(),
        )),
        tag_assignment_repo: Arc::new(SqliteTagAssignmentRepo::new(inner_pool.clone())),
        alert_policy_repo: Arc::new(SqliteAlertPolicyRepo::new(inner_pool.clone())),
        notification_connection_repo: Arc::new(SqliteNotificationConnectionRepo::new(
            inner_pool.clone(),
        )),
        seed_repo,
    };

    tracing::info!("SQLite 后端初始化完成");
    Ok(ctx)
}

// ============================================================================
// PostgreSQL 后端初始化
// ============================================================================

#[cfg(feature = "postgres")]
async fn init_postgres(
    database_url: &str,
    seed_mode: SeedMode<'_>,
) -> anyhow::Result<BackendContext> {
    use dts_backend_postgres::{
        PgAlertPolicyRepo, PgAlertRepo, PgBiShareRepo, PgDataAccessRepo, PgDatasourceRepo,
        PgDevelopmentRevisionRepo, PgMasterDataRepo, PgMessageDeliveryRepo, PgMetadataCatalogRepo,
        PgMigrationRunner, PgMonitoringRepo, PgNavigationRepo, PgNotificationConnectionRepo,
        PgOrganizationMembersRepo, PgPlatformProjectionRepo, PgProductJobRepo,
        PgProductResourceRepo, PgQualityAlertRepo, PgRoleRepo, PgRuntimeConfigurationRepo,
        PgSeedRepo, PgSensitiveFindingRepo, PgServiceAccessRepo, PgStandardGovernanceRepo,
        PgTagAssignmentRepo, PgTaskLogRepo, PgTaskRepo, PgTaskRunRepo, PgTenantRepo, PgUserRepo,
    };

    // 1. 创建连接池
    let pool = dts_backend_postgres::create_pool(database_url).await?;

    // 2. 运行迁移
    let inner_pool = pool.inner().clone();
    let migration_runner = PgMigrationRunner::new(inner_pool.clone());
    migration_runner.run_migrations().await?;

    // 3. 种子数据
    let seed_repo = Arc::new(PgSeedRepo::new(inner_pool.clone()));
    initialize_seed(seed_repo.as_ref(), seed_mode).await?;

    // 4. 装配 Repository
    let ctx = BackendContext {
        pool: Arc::new(pool),
        tenant_repo: Arc::new(PgTenantRepo::new(inner_pool.clone())),
        user_repo: Arc::new(PgUserRepo::new(inner_pool.clone())),
        role_repo: Arc::new(PgRoleRepo::new(inner_pool.clone())),
        datasource_repo: Arc::new(PgDatasourceRepo::new(inner_pool.clone())),
        task_repo: Arc::new(PgTaskRepo::new(inner_pool.clone())),
        task_run_repo: Arc::new(PgTaskRunRepo::new(inner_pool.clone())),
        task_log_repo: Arc::new(PgTaskLogRepo::new(inner_pool.clone())),
        alert_repo: Arc::new(PgAlertRepo::new(inner_pool.clone())),
        monitoring_repo: Arc::new(PgMonitoringRepo::new(inner_pool.clone())),
        product_resource_repo: Arc::new(PgProductResourceRepo::new(inner_pool.clone())),
        product_job_repo: Arc::new(PgProductJobRepo::new(inner_pool.clone())),
        message_delivery_repo: Arc::new(PgMessageDeliveryRepo::new(inner_pool.clone())),
        master_data_repo: Arc::new(PgMasterDataRepo::new(inner_pool.clone())),
        metadata_catalog_repo: Arc::new(PgMetadataCatalogRepo::new(inner_pool.clone())),
        metadata_lineage_repo: Arc::new(PgProductResourceRepo::new(inner_pool.clone())),
        data_access_repo: Arc::new(PgDataAccessRepo::new(inner_pool.clone())),
        assets_repo: Arc::new(PgProductResourceRepo::new(inner_pool.clone())),
        service_access_repo: Arc::new(PgServiceAccessRepo::new(inner_pool.clone())),
        bi_share_repo: Arc::new(PgBiShareRepo::new(inner_pool.clone())),
        sensitive_finding_repo: Arc::new(PgSensitiveFindingRepo::new(inner_pool.clone())),
        navigation_repo: Arc::new(PgNavigationRepo::new(inner_pool.clone())),
        organization_members_repo: Arc::new(PgOrganizationMembersRepo::new(inner_pool.clone())),
        platform_projection_repo: Arc::new(PgPlatformProjectionRepo::new(inner_pool.clone())),
        quality_alert_repo: Arc::new(PgQualityAlertRepo::new(inner_pool.clone())),
        standard_governance_repo: Arc::new(PgStandardGovernanceRepo::new(inner_pool.clone())),
        development_revision_repo: Arc::new(PgDevelopmentRevisionRepo::new(inner_pool.clone())),
        runtime_configuration_repo: Arc::new(PgRuntimeConfigurationRepo::new(inner_pool.clone())),
        tag_assignment_repo: Arc::new(PgTagAssignmentRepo::new(inner_pool.clone())),
        alert_policy_repo: Arc::new(PgAlertPolicyRepo::new(inner_pool.clone())),
        notification_connection_repo: Arc::new(PgNotificationConnectionRepo::new(
            inner_pool.clone(),
        )),
        seed_repo,
    };

    tracing::info!("PostgreSQL 后端初始化完成");
    Ok(ctx)
}

#[cfg(all(test, feature = "sqlite"))]
mod tests;
