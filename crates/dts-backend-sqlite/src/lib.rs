//! DTS Backend SQLite — SQLite 协议存储后端
//!
//! 基于 sqlx 实现，用于本地开发和测试。
//! 使用 sqlite::memory: 模式可实现零配置测试。

pub mod pool;
pub mod repo;

mod migrate;

pub use migrate::SqliteMigrationRunner;
pub use pool::{SqliteBackendPool, create_pool};
pub use repo::{
    SqliteAlertRepo, SqliteDatasourceRepo, SqliteMonitoringRepo, SqliteRoleRepo, SqliteSeedRepo,
    SqliteTaskRepo, SqliteTenantRepo, SqliteUserRepo,
};
