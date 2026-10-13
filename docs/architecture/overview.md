# DTS Platform — 架构总览

## 项目简介

DTS Platform 面向中小企业，使用一个 Rust workspace 维护控制面、引擎适配和公共权限能力。控制面支持 SQLite/PostgreSQL 元数据存储；用户会话使用 Sa-Token，业务接口执行 RBAC 与租户资源检查。ape-dts 适配器当前支持 MySQL/PostgreSQL 全量追加任务，JiaScheduler 用于可选的周期调度，SwanLake 提供有界 Arrow Flight SQL 查询，DeerFlow 是可选的外部 AI 服务。

当前验证包含真实 SQLite、Sa 会话和本地 Flight 测试服务。真实 ape-dts 源库到目标库、生产 PostgreSQL、SwanLake 部署和多个控制面协作需要各自的环境验收。DuckDB sink、CDC、数据建模和治理等后续能力见 [产品交付范围](../product/axure-delivery-map.md)。查询业务数据必须与含用户、密码哈希和数据源凭据的控制面数据库隔离。

## 技术愿景

以轻量部署、清晰的失败与恢复语义、实际业务闭环为目标。吞吐量、内存和硬件要求通过可重复基准测量后再对外说明。分布式任务调度与单条 SQL 的分布式执行分别定义和验收。

## 整体架构

```
┌─────────────────────────────────────────────────────┐
│                   Vite + React                      │
│  (SPA, Tailwind CSS + CSS Variables, 中英双语)        │
│  现有业务路由 + 深浅色主题切换                         │
└─────────────────────┬───────────────────────────────┘
                      │ REST API + WebSocket
┌─────────────────────┴───────────────────────────────┐
│               dts-server (actix-web)                 │
│  ├── Auth (Sa-Token + RBAC + 多租户)                  │
│  ├── API Routes (任务/数据源/监控/告警/调度)            │
│  ├── Service Layer (厚业务逻辑)                        │
│  ├── Engine Service (trait 抽象 → ape-dts CLI)        │
│  ├── Scheduler Client (jiascheduler HTTP)            │
│  └── WsHub (实时日志/进度推送)                         │
│         │                                           │
│         ├── 调用 ape-dts (CLI 子进程)                  │
│         └── 多后端存储 (SQLite dev / PostgreSQL prod)   │
└─────────────────────────────────────────────────────┘
                      │
┌─────────────────────┴───────────────────────────────┐
│              控制面存储与共享领域类型                    │
│  dts-backend (traits) → dts-backend-rdbms (公共)     │
│  → dts-backend-sqlite / dts-backend-postgres        │
│  运行时通过 DATABASE_BACKEND 环境变量派发              │
└─────────────────────────────────────────────────────┘
                      │
┌─────────────────────┴───────────────────────────────┐
│              外部集成组件                               │
│  ape-dts (CLI 子进程) + jiascheduler (HTTP)           │
│  SwanLake (Flight SQL) + 可选 DeerFlow 服务            │
└─────────────────────────────────────────────────────┘
```

## 分层架构

```
┌──────────────────────────────────────────────────────┐
│                   API Layer (handlers)               │
│  薄层: 仅做参数验证、序列化、调用 Service             │
├──────────────────────────────────────────────────────┤
│                  Service Layer                       │
│  厚层: 所有业务逻辑在此                              │
│  - TaskService: 任务 CRUD + 启停 + 调度               │
│  - TenantService: 租户创建与隔离                       │
│  - AuthService: Sa-Token 会话 + RBAC 权限判断         │
│  - EngineService: 与引擎交互 (trait 抽象)              │
├──────────────────────────────────────────────────────┤
│               Repository Layer (DAO)                 │
│  SQL 操作封装, 与数据库一一对应                       │
│  每个 Repo 实现对应的 trait, 方便 mock 测试           │
├──────────────────────────────────────────────────────┤
│             Infrastructure Layer                     │
│  - 数据库连接池                                       │
│  - 外部引擎的 CLI / HTTP / Flight 适配                 │
│  - 配置管理                                          │
│  - 日志/指标/追踪                                     │
└──────────────────────────────────────────────────────┘
```

## 核心设计原则

1. **依赖倒置**：上层依赖 trait，不依赖具体实现
2. **单一职责**：每个文件聚焦一个功能领域，CI 限制 Rust 文件不超过 800 行
3. **EngineService trait**：定义 `submit_task()`、`cancel_task()`、`get_status()` 接口，单机版和分布式版各自实现
4. **显式状态**：执行与取消状态由持久化记录及引擎确认驱动，不以提交请求代替执行成功
5. **错误显式处理**：使用 `thiserror` + `anyhow`，不静默吞掉错误

## 相关文档

- [数据库后端架构设计](./database-backend.md) — 多后端存储的详细设计（trait 层次、feature flags、迁移策略、如何新增后端）
- [单 workspace 的模块边界](../adr/0001-modular-workspace.md) — 权限底座、数据业务与未来 worker 的推荐分工
- [Sa-Token 会话决策](../adr/0002-sa-token-authentication.md) — 存储模式、权限数据来源与升级边界

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 前端 | Vite + React + TypeScript | SPA 仪表盘 |
| UI 样式 | Tailwind CSS + CSS Variables | 深浅色主题系统 |
| 国际化 | react-i18next | 中英双语 (zh-CN / en-US) |
| 状态管理 | Zustand + TanStack Query | 客户端 + 服务端状态分离 |
| 图表 | Recharts | 监控图表 |
| 后端 | actix-web 4.x | Rust Web 框架 |
| DB 驱动 | sqlx 0.8.x | 异步 SQL 工具 |
| 数据库 | SQLite (开发默认) | 零配置、文件存储、内存测试 |
| 数据库 (生产) | PostgreSQL | 运行时 Feature Flag 切换 |
| 密码哈希 | argon2 | 安全密码存储 |
| 会话认证 | sa-token-rust 0.2.0 | 可撤销会话、数据库实时授权 |
| 引擎 | ape-dts (git dep) | CLI 子进程模式调用 |
| 调度 | jiascheduler (HTTP client) | 分布式调度引擎 |
| 日志 | tracing | 结构化日志 |

## Phase 演进路线

### Phase 1 — 单机可用 ✅
- dts-server: REST API + Sa-Token + RBAC 多租户
- 多后端存储：SQLite (开发) + PostgreSQL (生产)，运行时 Feature Flag 切换
- 完整前端 (6 个页面) + 中英双语 + 深浅色主题
- 数据库 trait 抽象层 (8 个 Repository trait)

### Phase 2 — 引擎集成 ✅
- EngineService trait 抽象 + ape-dts CLI 子进程适配器
- TaskScheduler + TaskRunRepo
- WebSocket 实时日志/进度推送 (WsHub)
- Service 层重构 + tracing 结构化日志
- jiascheduler 调度集成 (dts-scheduler-client)

### Phase 3 — AI + 分析 (🚧 进行中)
- DeerFlow AI 集成 (AI 助手浮层面板)
- SwanLake 分析引擎集成 (Arrow Flight SQL)
- 智能分析页 (NL2SQL + SQL 工作台 + 报告生成)
- 分布式 Coordinator-Worker + K8s
