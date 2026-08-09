# DTS Platform — 轻量级数据中台

基于 [ape-dts](https://github.com/apecloud/ape-dts) 数据同步引擎构建的轻量级数据中台，整合集成引擎 (ape-dts)、调度引擎 (jiascheduler)、分析引擎 (SwanLake)、AI 智能体 (DeerFlow)，提供统一门户的数据接入、任务调度、智能分析与 AI 驱动能力。

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Vite + React + TypeScript + Tailwind CSS |
| 后端 | Rust (actix-web 4, sqlx 0.8) |
| 数据库 | SQLite (开发默认) + PostgreSQL (运行时切换) |
| 认证 | JWT + argon2 密码哈希 |
| 引擎 | ape-dts (CLI 子进程模式) |
| 调度 | jiascheduler (HTTP client) |
| i18n | 中英双语 (react-i18next) |
| 主题 | CSS Variables 深浅色切换 |

## 快速开始

```bash
# 1. 环境准备
cp .env.example .env
# 编辑 .env 修改 JWT_SECRET 等配置

# 2. 创建数据库 (SQLite)
python -c "
import sqlite3, os
os.makedirs('./data', exist_ok=True)
conn = sqlite3.connect('./data/dts.db')
conn.execute('PRAGMA journal_mode=WAL')
with open('crates/dts-server/migrations/001_initial_schema.sql', encoding='utf-8') as f:
    conn.executescript(f.read())
conn.close()
"

# 3. 启动后端
DATABASE_URL="sqlite:./data/dts.db" cargo run -p dts-server

# 4. 启动前端 (另一个终端)
cd web && npm install && npm run dev
```

访问 http://localhost:5173 登录：
- 用户名: `admin`
- 密码: `admin123`
- 租户: `default`

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
│       │   ├── middleware/     # JWT 认证 + 租户隔离
│       │   └── ws/             # WebSocket (实时日志/进度)
│       └── migrations/         # 数据库迁移 SQL
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

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 用户登录 |
| GET | `/api/v1/auth/me` | 当前用户信息 |
| POST | `/api/v1/tenants` | 创建租户 |
| GET | `/api/v1/tenants` | 租户列表 |
| GET | `/api/v1/datasources` | 数据源列表 |
| POST | `/api/v1/datasources` | 添加数据源 |
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
| tenant_admin | 全部权限 (含用户管理) |
| developer | 任务 + 数据源 + 监控 |
| viewer | 只读 |

## 演进路线

- **Phase 1** (✅ 完成): 单机版，SQLite + REST API + RBAC 多租户 + 前端
- **Phase 2** (✅ 完成): ape-dts 引擎集成 + TaskScheduler + TaskRunRepo + Service 层重构 + 多后端存储 (SQLite/PostgreSQL) + i18n + 主题系统
- **Phase 3** (🚧 进行中): 分布式 Coordinator-Worker、DeerFlow AI 集成、SwanLake 分析引擎、K8s
