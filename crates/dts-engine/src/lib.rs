//! DTS Engine — 数据同步引擎抽象层
//!
//! 定义 `EngineService` trait，作为不同同步引擎实现（ape-dts、自研等）的统一抽象。
//! 业务层通过此 trait 提交同步任务、取消运行，并通过 channel 接收进度反馈。

pub mod error;
pub mod model;

pub use error::EngineError;
pub use model::*;

/// 引擎抽象 trait
///
/// 不同引擎实现只需满足此接口即可接入 DTS Platform。
#[async_trait::async_trait]
pub trait EngineService: Send + Sync {
    /// 提交同步任务，返回 `run_id`
    ///
    /// 引擎通过 `progress_tx` 向调用方推送实时进度。
    /// 返回的 `run_id` 用于后续查询或取消任务。
    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError>;

    /// 取消正在运行的任务
    async fn cancel(&self, run_id: &str) -> Result<(), EngineError>;
}
