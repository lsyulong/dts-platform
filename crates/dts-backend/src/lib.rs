//! DTS Backend — 存储后端核心抽象层
//!
//! 定义所有存储后端必须实现的 trait 和公共类型。
//! 实现者只需遵循 trait 约定即可让 dts-platform 支持新的数据库协议。
//!
//! # 架构
//!
//! ```text
//! dts-server (应用层) → dts-backend (trait 层) → dts-backend-postgres / mysql / oracle ...
//! ```

pub mod error;
pub mod pool;
pub mod repo;

// Re-export types from dts-core (backward compatible)
pub use dts_core::*;

pub use error::BackendError;
pub use pool::{BackendPool, BackendPoolFactory};
pub use repo::{
    AlertRepo, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo, TenantRepo, UserRepo,
};
