# DTS Platform — 轻量级数据中台

基于 [ape-dts](https://github.com/apecloud/ape-dts) 数据同步引擎构建的轻量级数据中台，整合集成引擎 (ape-dts)、调度引擎 (jiascheduler)、分析引擎 (SwanLake)、AI 智能体 (DeerFlow)，提供统一门户的数据接入、任务调度、智能分析与 AI 驱动能力。

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Vite + React + TypeScript + Tailwind CSS |
| 后端 | Rust (actix-web 4, sqlx 0.8) |
| 数据库 | SQLite (开发默认) + PostgreSQL (运行时切换) |
| 认证 | sa-token-rust 会话认证 + argon2 密码哈希 |
| 引擎 | ape-dts (CLI 子进程模式) |
| 调度 | jiascheduler (HTTP client) |
| i18n | 中英双语 (react-i18next) |
| 主题 | CSS Variables 深浅色切换 |

## 快速开始

```powershell
# 1. 环境准备
if (-not (Test-Path -LiteralPath .env)) { Copy-Item .env.example .env }
# 编辑 .env：设置首次管理员密码（至少12字节），按需调整会话有效期。

# 2. 创建数据库 (SQLite)
New-Item -ItemType Directory -Path data -Force | Out-Null
# 迁移由后端自动执行，无需手动导入 SQL。

# 3. 启动后端
cargo run -p dts-server

# 4. 启动前端 (另一个终端)
Set-Location web
# 首次克隆项目时运行 npm ci；已有依赖无需重复安装。
npm run dev
```

访问 http://localhost:5173 登录：
- 用户名: `admin`
- 密码: `.env` 中首次设置的 `BOOTSTRAP_ADMIN_PASSWORD`
- 租户: `default`

登录使用 sa-token-rust 0.2.0 的不透明随机令牌，API 通过 `Authorization: Bearer <token>`
传递；退出会撤销服务端会话。升级后原 JWT 不再作为登录凭据接受，需要重新登录。
默认 `AUTH_STORAGE=memory`，同一服务中的 Actix workers 共享一个会话管理器；服务重启后
需要重新登录。多实例会话共享可用已有 Redis，设置 `AUTH_STORAGE=redis`、
`AUTH_REDIS_URL` 并构建 `auth-redis` feature。共享认证会话不代表已实现多控制面任务协调。

### 既有数据库升级后恢复平台管理员

升级会保留已有账户、密码和角色，不会把旧 `tenant_admin` 自动提升为平台所有者。
旧管理员仍可管理自己的租户，但创建新租户需要 `platform_admin` 的 `tenant:create` 权限。
若旧部署还没有平台所有者，请由具有数据库及部署主机管理权限的操作员进行一次离线授权：

1. 备份现有数据库，从已有用户列表 API `/api/v1/system/users` 或数据库 `user` 表核对目标用户的真实 ID、所属租户和 active 状态。
2. 停止平台 HTTP 服务，确认当前 `.env` 的 `DATABASE_URL`、`DATABASE_BACKEND` 指向要升级的数据库。
3. 使用已构建的后端可执行文件执行下面命令，将参数替换为已核对的真实用户 ID。示例为本地 debug 构建路径，发行部署使用其已安装的同名可执行文件。

```powershell
.\target\debug\dts-server.exe --grant-platform-admin "已核对的 user_id"
```

命令只给明确选定的现有 active 用户增加 `platform_admin` 系统角色，保留原有角色及密码；
成功后退出，不启动 HTTP 服务，也不需要设置 `BOOTSTRAP_ADMIN_PASSWORD`。
重复执行会复用已验证的角色和授权。目标用户或租户停用时会拒绝执行；同名普通角色或权限
不完整的系统角色也会报错，请先核查角色数据，命令不会自动扩大这些角色的权限。
完成后重新启动平台，并让该用户重新登录；升级前的 JWT 仍需按上面的说明重新登录获取会话。

## 项目结构

```
dts-platform/
├── crates/
│   ├── dts-core/               # 共享类型定义 (DB-agnostic)
│   ├── dts-backend/            # Repository trait 定义 + domain types
│   ├── dts-backend-rdbms/      # RDBMS 公共层 (错误映射 + 密码工具)
│   ├── dts-backend-sqlite/     # SQLite 后端实现 (开发默认)
│   ├── dts-backend-postgres/   # PostgreSQL 后端实现 (生产推荐)
│   ├── dts-engine/             # 引擎 trait 定义
│   ├── dts-engine-ape/         # ape-dts 引擎适配器 (CLI 子进程)
│   ├── dts-scheduler-client/   # jiascheduler HTTP 客户端
│   └── dts-server/             # Rust 后端 (actix-web)
│       ├── src/
│       │   ├── main.rs         # 启动入口
│       │   ├── app.rs          # actix-web 路由配置
│       │   ├── config.rs       # 配置管理 + 运行时后端派发
│       │   ├── db/             # 数据库连接 + 迁移 + 种子数据
│       │   ├── models/         # 数据模型 + 错误类型
│       │   ├── api/            # REST API handlers (薄层)
│       │   ├── services/       # 业务逻辑 (厚层)
│       │   ├── middleware/     # Sa-Token 认证 + 租户隔离
│       │   └── ws/             # WebSocket (实时日志/进度)
│                               # 迁移 SQL 位于对应存储后端 crate
├── web/                        # Vite + React 前端
│   └── src/
│       ├── layouts/            # 布局组件
│       ├── pages/              # 6 个业务页面
│       ├── components/         # 可复用组件
│       ├── api/                # API 调用封装
│       ├── stores/             # Zustand 状态管理
│       ├── i18n/               # 国际化配置
│       └── locales/            # 中英文翻译文件
└── docs/                       # 架构设计文档
    └── architecture/           # 架构总览 + 数据模型 + 多后端 + 引擎集成
```

## API 端点

JIA 定时调度需要在 `.env` 设置 `JIA_ENABLED=true`、调度服务登录信息、至少 32 字节的
`JIA_SERVICE_TOKEN`、单个 `JIA_INSTANCE_ID` 和 `JIA_CALLBACK_BASE_URL`。实例 ID 来自 JIA
已注册且在线的 Linux worker，该节点需要 `bash` 和 `curl`。回调 URL 必须是 worker
能够访问的 DTS 管理 API 地址，例如 `https://dts.example.com`；跨主机部署不能用 worker
自己的 `localhost`，也不能使用监听地址 `0.0.0.0`。兼容旧环境变量 `JIA_CALLBACK_URL`，
两者同时配置时以 `JIA_CALLBACK_BASE_URL` 为准。

在任务详情设置五字段 Unix cron，或含秒的六字段 cron；调度时区固定 **UTC**。
平台保存 JIA 的定时模板后，还会向指定节点下发 `start_timer`；更新时先停止旧调度、
刷新执行快照再启动，取消时先停止 worker 再删除模板。任何下发错误都会返回失败。
状态中的 `has_timer` 仅表示模板存在，`active` 需要 JIA 确认节点在线并处于
`scheduling`；`pending`、`offline`、`stopped` 不表示调度正常运行。调度下发记录与
Pipeline 的执行记录分开，HTTP 触发成功不能证明数据已同步完成。调度使用单个指定
节点，数据同步目前仍在 DTS 服务进程启动的 ape-dts 中执行，尚无分布式数据执行或
exactly-once 保证。真实 JIA + 数据库环境的端到端验收仍需完成。

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 用户登录 |
| POST | `/api/v1/auth/logout` | 撤销当前登录会话 |
| GET | `/api/v1/auth/me` | 当前用户信息 |
| POST | `/api/v1/tenants` | 创建租户 |
| GET | `/api/v1/tenants` | 租户列表 |
| GET | `/api/v1/datasources` | 数据源列表 |
| POST | `/api/v1/datasources` | 添加数据源 |
| POST | `/api/v1/datasources/:id/test` | 执行并记录真实连接检测 |
| GET | `/api/v1/datasources/:id/probes` | 查询连接检测历史 |
| DELETE | `/api/v1/datasources/:id` | 删除数据源 |
| GET | `/api/v1/tasks` | 任务列表 |
| POST | `/api/v1/tasks` | 创建任务 |
| GET | `/api/v1/tasks/:id` | 任务详情 |
| PATCH | `/api/v1/tasks/:id/status` | 更新任务状态 |
| GET | `/api/v1/monitoring/overview` | 监控概览 |
| GET | `/api/v1/alerts` | 告警规则列表 |
| POST | `/api/v1/alerts` | 创建告警规则 |
| PATCH | `/api/v1/alerts/:id/toggle` | 切换告警状态 |
| POST | `/api/v1/ai/chat` | AI 对话 (SSE 流) |
| POST | `/api/v1/ai/diagnose` | AI 故障诊断 (SSE 流) |
| WS | `/ws/tasks/:id` | 任务实时日志 WebSocket |

## RBAC 权限

| 角色 | 权限范围 |
|------|---------|
| platform_admin | 全部 canonical 权限，含创建租户；其余业务操作仍遵守租户边界 |
| tenant_admin | 当前租户的任务、数据源、监控、告警及用户管理，不含创建租户 |
| developer | 任务 + 数据源 + 监控 |
| viewer | 只读 |

## 演进路线

- **Phase 1** (✅ 完成): 单机版，SQLite + REST API + RBAC 多租户 + 前端
- **Phase 2** (✅ 完成): ape-dts 引擎集成 + TaskScheduler + TaskRunRepo + Service 层重构 + 多后端存储 (SQLite/PostgreSQL) + i18n + 主题系统
- **Phase 3** (🚧 进行中): 分布式 Coordinator-Worker、DeerFlow AI 集成、SwanLake 分析引擎、K8s
