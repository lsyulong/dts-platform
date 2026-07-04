# DTS Platform — 开发环境搭建

## 前置要求

- **Rust** 1.85.0+ (与 ape-dts 保持一致)
- **Node.js** 20.x+
- **pnpm** 9.x+
- **Git**

## 快速开始

```bash
# 1. 克隆项目
git clone <repo-url> dts-platform
cd dts-platform

# 2. 启动后端
cd crates/dts-server
cargo run
# 后端启动在 http://localhost:8080

# 3. 启动前端 (另一个终端)
cd web
pnpm install
pnpm dev
# 前端启动在 http://localhost:5173
```

## 项目结构

```
dts-platform/
├── crates/
│   └── dts-server/        # Rust 后端
│       ├── src/
│       │   ├── main.rs     # 启动入口
│       │   ├── lib.rs
│       │   ├── app.rs      # actix-web App 配置
│       │   ├── config.rs   # 配置管理
│       │   ├── db/         # 数据库连接 + 迁移
│       │   ├── models/     # 数据模型
│       │   ├── api/        # REST handlers
│       │   ├── services/   # 业务逻辑
│       │   ├── middleware/  # JWT + 租户隔离
│       │   └── ws/         # WebSocket
│       └── Cargo.toml
├── web/                    # Vite + React 前端
│   ├── src/
│   │   ├── layouts/        # 布局组件
│   │   ├── pages/          # 页面组件
│   │   ├── components/     # 可复用组件
│   │   ├── hooks/          # 自定义 hooks
│   │   ├── api/            # API 调用
│   │   ├── stores/         # Zustand 状态管理
│   │   └── lib/            # 工具函数
│   ├── package.json
│   └── vite.config.ts
└── docs/                   # 文档
    ├── architecture/       # 架构设计
    ├── api/               # API 规范
    └── development/       # 开发指南
```

## 环境变量

```bash
# .env (后端)
DATABASE_URL=sqlite:./data/dts.db?mode=rwc
JWT_SECRET=your-secret-key-change-in-production
JWT_EXPIRY_HOURS=24
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
RUST_LOG=info,dts_server=debug

# .env.local (前端)
VITE_API_BASE_URL=http://localhost:8080/api/v1
VITE_WS_URL=ws://localhost:8080/ws
```

## 常用命令

```bash
# 后端
cargo run -p dts-server              # 启动服务
cargo test -p dts-server             # 运行测试
cargo clippy -p dts-server           # 代码检查
cargo fmt --all                      # 代码格式化

# 前端
cd web && pnpm dev                   # 启动开发服务器
cd web && pnpm build                 # 生产构建
cd web && pnpm lint                  # ESLint 检查
cd web && pnpm typecheck             # TypeScript 检查
```

## 数据库迁移

SQLite 使用 `sqlx::migrate!` 宏进行迁移：

```bash
# 创建新迁移
# 在 crates/dts-server/migrations/ 下创建 SQL 文件

# 运行迁移 (应用启动时自动执行)
cargo run -p dts-server
```

## 测试

```bash
# 后端单元测试
cargo test -p dts-server

# 后端集成测试
cargo test -p dts-server -- --ignored

# 前端测试
cd web && pnpm test
```
