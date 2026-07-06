//! DTS Backend PostgreSQL — PostgreSQL 协议存储后端
//!
//! 基于 sqlx 实现，兼容所有 PostgreSQL 协议数据库：
//! - PostgreSQL / Supabase
//! - GaussDB / openGauss（华为）
//! - Kingbase（人大金仓）
//! - HighGo（瀚高）
//! - CockroachDB / YugabyteDB
//! - Greenplum / Redshift

pub mod pool;
pub mod repo;

mod migrate;

pub use migrate::PgMigrationRunner;
pub use pool::{PgBackendPool, create_pool};
pub use repo::{
    PgAlertRepo, PgDatasourceRepo, PgMonitoringRepo, PgRoleRepo, PgSeedRepo, PgTaskRepo,
    PgTenantRepo, PgUserRepo,
};
