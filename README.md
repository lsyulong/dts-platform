# DTS Platform — 企业级数据同步管理平台

基于 [ape-dts](https://github.com/apecloud/ape-dts) 数据同步引擎构建的企业级 Web 管理平台。

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Vite + React + TypeScript + Tailwind CSS |
| 后端 | Rust (actix-web) |
| 数据库 | SQLite (Phase 1) → Supabase/PostgreSQL (Phase 3) |
| 认证 | JWT + argon2 密码哈希 |
| 引擎 | ape-dts (Git 依赖) |

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
├── crates/dts-server/       # Rust 后端
│   ├── src/
│   │   ├── main.rs          # 启动入口
│   │   ├── app.rs           # actix-web 路由配置
│   │   ├── config.rs        # 配置管理
│   │   ├── db/              # 数据库连接 + 迁移 + 种子数据
│   │   ├── models/          # 数据模型 + 错误类型
│   │   ├── api/             # REST API handlers
│   │   ├── services/        # 业务逻辑
│   │   ├── middleware/       # JWT 认证 + 租户隔离
│   │   └── ws/              # WebSocket (实时日志/进度)
│   └── migrations/          # 数据库迁移 SQL
├── web/                     # Vite + React 前端
│   └── src/
│       ├── layouts/         # 布局组件
│       ├── pages/           # 6 个业务页面
│       ├── components/      # 可复用组件
│       ├── api/             # API 调用封装
│       └── stores/          # Zustand 状态管理
└── docs/                    # 技术文档
    ├── architecture/        # 架构设计 + 数据模型 + 引擎集成
    └── development/         # 开发指南 + 实现计划
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
| WS | `/ws/tasks/:id` | 任务实时日志 WebSocket |

## RBAC 权限

| 角色 | 权限范围 |
|------|---------|
| tenant_admin | 全部权限 (含用户管理) |
| developer | 任务 + 数据源 + 监控 |
| viewer | 只读 |

## 演进路线

- **Phase 1** (已完成): 单机版，SQLite + REST API + 前端
- **Phase 2** (计划中): ape-dts 引擎集成，实时进度推送
- **Phase 3** (计划中): 分布式 Coordinator-Worker，PostgreSQL，K8s
