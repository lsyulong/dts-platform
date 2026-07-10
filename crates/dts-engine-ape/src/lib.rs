//! DTS Engine APE — ape-dts 引擎封装
//!
//! 实现 `dts_engine::EngineService` trait，将 ape-dts 库接入 DTS Platform。
//!
//! 当前为骨架实现：`ApeEngineService` 的 `submit` 通过 tokio::spawn 异步启动
//! 同步任务，进度通过 channel 回传。实际的 ape-dts 调用标记为 TODO。

mod adapter;
mod service;

pub use service::ApeEngineService;
