# DTS Platform — ape-dts 引擎集成设计

## 集成方式

ape-dts 通过 Git 依赖引入，作为 Rust library 直接调用，不走子进程。

```toml
# crates/dts-server/Cargo.toml
[dependencies]
dt-common = { git = "https://github.com/guyulong/ape-dts" }
dt-task = { git = "https://github.com/guyulong/ape-dts" }
dt-connector = { git = "https://github.com/guyulong/ape-dts" }
```

## EngineService Trait 设计

扩展性的关键：通过 trait 抽象引擎调用，单机版和分布式版各自实现，业务层无需改动。

```rust
use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub task_id: String,
    pub source: DatasourceInfo,
    pub target: DatasourceInfo,
    pub table_mappings: Vec<TableMapping>,
    pub filter_config: Option<FilterConfig>,
    pub parallel_config: Option<ParallelConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_run_id: String,
    pub status: TaskStatus,
    pub rows_synced: u64,
    pub bytes_synced: u64,
    pub total_rows: u64,
    pub total_bytes: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum TaskStatus {
    Draft,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterHealth {
    pub total_workers: u32,
    pub active_workers: u32,
    pub running_tasks: u32,
}

#[async_trait]
pub trait EngineService: Send + Sync {
    /// 提交同步任务
    async fn submit_task(&self, config: TaskConfig) -> Result<String>;

    /// 取消运行中的任务
    async fn cancel_task(&self, task_run_id: &str) -> Result<()>;

    /// 暂停任务
    async fn pause_task(&self, task_run_id: &str) -> Result<()>;

    /// 恢复暂停的任务
    async fn resume_task(&self, task_run_id: &str) -> Result<()>;

    /// 获取任务当前状态
    async fn get_status(&self, task_run_id: &str) -> Result<TaskStatus>;

    /// 订阅任务实时日志
    async fn subscribe_logs(&self, task_run_id: &str) -> Result<mpsc::Receiver<LogEntry>>;

    /// 订阅任务进度更新
    async fn subscribe_progress(&self, task_run_id: &str) -> Result<mpsc::Receiver<TaskProgress>>;

    /// 集群健康检查
    async fn health_check(&self) -> Result<ClusterHealth>;
}
```

## Phase 1 — LocalEngineService (单机实现)

直接调用 `dt-task::TaskRunner`，通过 `tokio::sync::mpsc::channel` 传递进度更新和日志。

```rust
pub struct LocalEngineService {
    active_tasks: Arc<DashMap<String, TaskHandle>>,
}

struct TaskHandle {
    cancel_tx: tokio::sync::oneshot::Sender<()>,
    progress_tx: tokio::sync::broadcast::Sender<TaskProgress>,
    log_tx: tokio::sync::broadcast::Sender<LogEntry>,
}
```

关键流程：
1. `submit_task` 将前端传来的 TaskConfig 转成 INI 配置格式
2. 通过 `tokio::spawn` 异步启动 TaskRunner
3. 在引擎执行过程中通过 channel 收集进度和日志
4. 通过 WebSocket 推送给前端

## Phase 3 — DistributedEngineService (分布式实现)

通过 NATS 消息队列与 Worker 通信：

```rust
pub struct DistributedEngineService {
    nats_client: async_nats::Client,
}

// 消息格式 (NATS subjects)
// dts.task.submit     → 提交任务
// dts.task.{id}.status → 任务状态更新
// dts.task.{id}.log    → 实时日志
// dts.task.{id}.cancel → 取消任务
// dts.worker.{id}.heartbeat → Worker 心跳
```

## 上游贡献切入点

以下增强点适合贡献给 ape-dts 上游：

1. **进度回调 API**：当前 dt-task 的 TaskRunner 缺少对外暴露进度/日志的接口，贡献一个 callback-based 的 API
2. **任务暂停/恢复**：增加 pause/resume 支持
3. **JSON/YAML 配置支持**：除 INI 外，支持 JSON/YAML 格式的任务配置
