# DTS Platform 工程优化 & ape-dts 引擎集成 — 设计文档

> 状态: 已确认 | 日期: 2026-07-08 | 作者: Claude

## 1. 概述

分两阶段顺序执行：先工程优化把质量拉满，再接入 ape-dts 数据同步引擎。

### 1.1 目标

- **工程质量**：零 unwrap/expect/clone 警告，文件 ≤ 400 行，函数 ≤ 50 行，测试覆盖率 ≥ 85%
- **开发体验**：`cargo build` 默认 SQLite 零配置开发，生产 PostgreSQL
- **引擎集成**：Trait 抽象隔离 ape-dts，支持单机/分布式切换

### 1.2 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust (actix-web 4.x) |
| 存储 | SQLite (dev) → PostgreSQL (prod)，trait 抽象可插拔 |
| 引擎 | ape-dts (git dep)，通过 EngineService trait 隔离 |
| 日志 | tracing (替代 log) |
| 测试 | cargo-llvm-cov, rstest, mockall |

---

## 2. Crate 架构

### 2.1 最终 Crate 图

```
dts-platform/
├── crates/
│   ├── dts-core/                    # [NEW] 共享基础类型
│   ├── dts-backend/                 # [REFACTOR] Trait 抽象层
│   ├── dts-backend-rdbms/           # [KEEP] RDBMS 公共层
│   ├── dts-backend-sqlite/          # [NEW] SQLite 实现
│   ├── dts-backend-postgres/        # [KEEP] PostgreSQL 实现
│   ├── dts-engine/                  # [NEW - Phase 2] 引擎抽象
│   ├── dts-engine-ape/              # [NEW - Phase 2] ape-dts 封装
│   └── dts-server/                  # [REFACTOR] 应用层
```

### 2.2 Feature Flags / Profile 策略

```toml
# dts-server/Cargo.toml
[features]
default = ["sqlite"]
sqlite = ["dts-backend-sqlite", "dts-backend-rdbms"]
postgres = ["dts-backend-postgres", "dts-backend-rdbms"]

[profile.dev]
# default features = sqlite → cargo build 即可本地开发

[profile.release]
# 编译 sqlite + postgres 两个后端，生产按需切换
```

---

## 3. Service 层重构

### 3.1 目标 Service 清单

```
services/
├── mod.rs
├── auth_svc.rs          # [SPLIT] JWT token 管理
├── tenant_svc.rs        # [KEEP] 租户 CRUD
├── user_svc.rs          # [NEW] 用户 CRUD + 密码管理
├── datasource_svc.rs    # [NEW] 数据源 CRUD + 连接测试 + 密码加密
├── task_svc.rs          # [NEW] 任务 CRUD + 状态机 + 启停控制
├── alert_svc.rs         # [NEW] 告警规则 CRUD + 阈值判断
├── monitoring_svc.rs    # [NEW] 监控聚合逻辑
└── scheduler.rs         # [NEW - Phase 2] cron 调度 + 任务队列
```

### 3.2 调用链

```
API Handler (薄层) → Service (业务逻辑) → Repository trait (数据访问)

Before: handler → Arc<dyn Repo> 直接调用
After:  handler → Service → Arc<dyn Repo>
```

---

## 4. SQLite 后端

### 4.1 crate 结构

```
crates/dts-backend-sqlite/
├── src/
│   ├── lib.rs, pool.rs, migrate.rs
│   ├── repo/ (8 files)
│   └── migrations/001_initial_schema.sql
```

### 4.2 DDL 差异

| PostgreSQL | SQLite |
|-----------|--------|
| `TIMESTAMPTZ` | `TEXT` (ISO 8601) |
| `BOOLEAN` | `INTEGER NOT NULL DEFAULT 0` |
| `BIGINT` | `INTEGER` |
| `DOUBLE PRECISION` | `REAL` |

### 4.3 测试模式

集成测试使用 `sqlite::memory:` 模式，零配置 CI，进程退出自动销毁。

---

## 5. 工程质量标准（硬性 Gate）

### 5.1 Lint 规则

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
redundant_clone = "deny"
clone_on_copy = "deny"
manual_ok_or = "warn"
too_many_arguments = "warn"

[workspace.lints.rust]
unsafe_code = "deny"
unused_must_use = "deny"
unused_imports = "deny"
dead_code = "deny"
```

### 5.2 文件 & 函数约束

- 每个 `.rs` 文件 ≤ 400 行（CI 检查）
- 每个函数 ≤ 50 行
- 嵌套深度 ≤ 4 层

### 5.3 测试覆盖率

- 目标：≥ 85% line coverage
- CI gate：`cargo llvm-cov --fail-under-lines 85`
- 默认使用 SQLite `:memory:` 进行集成测试

### 5.4 可观测性

- `log` → `tracing` 迁移
- 关键路径加 `#[tracing::instrument]` span
- 结构化日志字段

---

## 6. ape-dts 引擎集成（Phase 2）

### 6.1 EngineService Trait

```rust
#[async_trait]
pub trait EngineService: Send + Sync {
    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError>;

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError>;
}
```

### 6.2 ape-dts 封装

- `dts-engine-ape` crate 实现 `EngineService`
- 引擎在独立 tokio task 中运行
- 进度通过 `mpsc::unbounded_channel` 回调
- 取消通过 `CancellationToken`

### 6.3 TaskScheduler

- 持有 `Arc<dyn EngineService>` + `Arc<dyn TaskRepo>` + `Arc<WsHub>`
- 手动触发：`start_task(id)` / `stop_task(run_id)`
- 定时触发：`tick()` 每分钟检查 cron 表达式

### 6.4 数据流

```
API → TaskService → TaskScheduler → EngineService.submit(task, tx)
                                         ↓
                              ape-dts (dt-task + dt-connector)
                              callback → mpsc channel
                                         ↓
                              on_progress() → DB + WebSocket → Frontend
```

---

## 7. Phase 1 待修复项

### 高优先级

- [ ] `dts-server/src/db/mod.rs` → BackendContext 硬编码 Pg* 改为 trait object + 运行时派发
- [ ] 创建 `dts-backend-sqlite` crate 含所有 8 个 Repo + 迁移
- [ ] 创建 `dts-core` crate，移入共享 types
- [ ] 所有 handler 插入 Service 层（补齐 6 个 Service）
- [ ] `auth_svc.rs` 拆分（JWT 逻辑抽到独立模块）
- [ ] `log` → `tracing` 全量迁移

### 中优先级

- [ ] `.clone()` 审查 — pool.clone() 等不可避免的加 `#[allow]`
- [ ] 硬编码字符串 → const / config
- [ ] `config.rs` debug 值提取为常量

### 测试补齐

- [ ] `dts-backend-sqlite`: 8 个 Repo 集成测试
- [ ] `dts-server` services: 单元测试（mock Repository）
- [ ] `dts-server` api: 集成测试（actix-web::test）
- [ ] 覆盖率达到 85%+

---

## 8. 实施顺序

### Phase 1: 工程优化

1. 创建 `dts-core` crate，迁移共享 types
2. 创建 `dts-backend-sqlite` crate，实现所有 Repo + 迁移
3. 重构 `dts-backend` 去 types 依赖
4. 重写 `dts-server/src/db/mod.rs` 支持运行时派发 SQLite/PG
5. 补齐 Service 层全部 8 个 Service
6. Handler 重构为薄层
7. `log` → `tracing` 迁移
8. 消除所有 clippy deny
9. 测试补齐到 85%+
10. CI 硬化

### Phase 2: 引擎接入

1. 创建 `dts-engine` crate (EngineService trait)
2. 创建 `dts-engine-ape` crate (ape-dts 封装)
3. 实现 TaskScheduler
4. 新增 `TaskRunRepo` trait + 实现（SQLite + PG）
5. WebSocket 实时进度推送
6. 全链路集成测试
