//! DTS Scheduler Client — jiascheduler REST API 客户端
//!
//! 提供 DTS Platform 与 jiascheduler 之间的集成能力：
//! - 登录认证 (session cookie)
//! - 作业管理 (create/update/delete)
//! - 定时器管理 (cron trigger)
//! - 下发执行 (dispatch)
//! - 执行历史查询
//! - 调度历史查询
//! - 任务同步 (自动同步 DTS 任务到 jiascheduler)

pub mod client;
pub mod models;
pub mod sync;

pub use client::JiaSchedulerClient;
pub use sync::TaskSync;
