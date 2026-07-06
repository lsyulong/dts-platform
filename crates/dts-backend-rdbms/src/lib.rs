//! DTS Backend RDBMS — 关系型数据库公共层
//!
//! 提供所有关系型数据库后端共用的工具：
//! - 错误映射（sqlx Error → BackendError）
//! - 迁移执行器 trait
//! - 通用连接池配置

pub mod error;
pub mod migrate;

pub use error::map_sqlx_error;
pub use migrate::MigrationRunner;
