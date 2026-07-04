# DTS Platform — 架构总览

## 项目简介

DTS Platform 是基于 ape-dts 数据同步引擎构建的企业级数据同步管理平台，提供 Web UI 管理界面、RBAC 权限控制、多租户隔离、任务调度与实时监控等企业级功能。

## 技术愿景

**性能目标**：基于 Rust 技术栈，在数据处理吞吐量和资源效率上超越 Java 生态的 FlinkCDC、SeaTunnel、Debezium，不是一个数据量级的产品。

## 整体架构

```
┌─────────────────────────────────────────────────────┐
│                   Vite + React                      │
│  (SPA, shadcn/ui + Tailwind, 部署到 Vercel)          │
│  参考现有 6 个 HTML 页面进行组件化重构                   │
└─────────────────────┬───────────────────────────────┘
                      │ REST API + WebSocket
┌─────────────────────┴───────────────────────────────┐
│               dts-server (actix-web)                 │
│  ├── Auth (JWT + RBAC + 多租户)                       │
│  ├── API Routes (任务/数据源/监控/告警)                 │
│  ├── 任务调度器 (cron)                                │
│  └── 实时推送 (WebSocket)                             │
│         │                                           │
│         ├── 调用 ape-dts (git 依赖)                   │
│         └── SQLite 持久化 (配置/历史/用户/租户)          │
└─────────────────────────────────────────────────────┘
                      │
┌─────────────────────┴───────────────────────────────┐
│              ape-dts 现有引擎                         │
│  dt-main / dt-task / dt-connector ...               │
│  (library 调用，通过 trait 抽象)                      │
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
│  - TenantService: 租户隔离 + 配额管理                 │
│  - AuthService: JWT + RBAC 权限判断                   │
│  - EngineService: 与引擎交互 (trait 抽象)              │
├──────────────────────────────────────────────────────┤
│               Repository Layer (DAO)                 │
│  SQL 操作封装, 与数据库一一对应                       │
│  每个 Repo 实现对应的 trait, 方便 mock 测试           │
├──────────────────────────────────────────────────────┤
│             Infrastructure Layer                     │
│  - 数据库连接池                                       │
│  - 消息队列客户端 (NATS，Phase 2)                      │
│  - 配置管理                                          │
│  - 日志/指标/追踪                                     │
└──────────────────────────────────────────────────────┘
```

## 核心设计原则

1. **依赖倒置**：上层依赖 trait，不依赖具体实现
2. **单一职责**：每个文件聚焦一个功能领域，文件不超过 400 行
3. **EngineService trait**：定义 `submit_task()`、`cancel_task()`、`get_status()` 接口，单机版和分布式版各自实现
4. **不可变模式**：始终创建新对象，不修改现有对象
5. **错误显式处理**：使用 `thiserror` + `anyhow`，不静默吞掉错误

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 前端 | Vite + React + TypeScript | SPA 仪表盘 |
| UI 框架 | shadcn/ui + Tailwind CSS | 组件库 + 样式 |
| 状态管理 | Zustand | 轻量状态管理 |
| 图表 | Recharts | 监控图表 |
| 表格 | TanStack Table | 任务/数据源表格 |
| 后端 | actix-web 4.x | Rust Web 框架 |
| ORM | sqlx 0.8.x | 异步 SQL 工具 |
| 数据库 | SQLite (Phase 1) | 嵌入式数据库 |
| 数据库 (Phase 2) | Supabase/PostgreSQL | 生产数据库 |
| 密码哈希 | argon2 | 安全密码存储 |
| JWT | jsonwebtoken | 无状态认证 |
| 引擎 | ape-dts (git dep) | 数据同步引擎 |

## Phase 演进路线

### Phase 1 — 单机可用 (本次实现)
- dts-server: API + 直接调用 dt-task library
- SQLite 数据库
- 完整前端 (6 个页面)
- 本地开发 + Vercel 部署前端

### Phase 2 — 引擎增强 (给上游贡献)
- 给 dt-task 增加进度回调接口
- WebSocket 实时日志/进度推送
- Prometheus metrics 完善

### Phase 3 — 分布式 (生产级扩展)
- EngineService trait 抽象
- Coordinator-Worker 拆分
- NATS 消息队列
- PostgreSQL/Supabase 替换 SQLite
- K8s Helm Chart
