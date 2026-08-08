# 数据库后端架构设计

## 概述

DTS Platform 采用**可插拔多后端存储架构**，通过 trait 抽象层实现数据库后端的可替换性。

### 设计目标

- **开发/测试默认使用 SQLite**：零配置、文件存储、内存模式可测，开发者无需安装外部数据库
- **生产环境推荐 PostgreSQL**：高性能、高并发、完整 ACID、PG 协议兼容
- **易于扩展**：新增后端只需实现 8 个 Repository trait + BackendPool trait

## 分层架构

```
┌──────────────────────────────────────────────────────────────┐
│                      dts-server                              │
│  应用层：通过 db::init_backend() 运行时选择后端               │
│  BackendContext { pool: Arc<dyn BackendPool>, repos... }     │
└──────────────┬──────────────────────────┬────────────────────┘
               │ Arc<dyn RepoTrait>       │ Arc<dyn RepoTrait>
┌──────────────┴──────────────────────────┴────────────────────┐
│                      dts-backend                             │
│  抽象层：trait 定义 + 共享类型                                │
│  BackendPool, BackendPoolFactory                             │
│  TenantRepo, UserRepo, RoleRepo, DatasourceRepo,             │
│  TaskRepo, AlertRepo, MonitoringRepo, SeedRepo               │
│  BackendError, 所有 domain types                             │
└──────────────┬───────────────────────────────────────────────┘
               │
┌──────────────┴───────────────────────────────────────────────┐
│                   dts-backend-rdbms                          │
│  RDBMS 公共层：错误映射 + 密码工具 + 迁移 trait               │
│  map_sqlx_error() — PG + MySQL + SQLite 错误码               │
│  hash_password_sync() — Argon2 密码哈希                      │
│  MigrationRunner trait                                       │
└──────┬─────────────────────────────────┬─────────────────────┘
       │                                 │
┌──────┴──────────────────┐  ┌──────────┴──────────────────────┐
│  dts-backend-sqlite     │  │  dts-backend-postgres            │
│  SQLite 实现 (开发默认)   │  │  PostgreSQL 实现 (生产推荐)       │
│                         │  │                                  │
│  SqliteBackendPool      │  │  PgBackendPool                   │
│  SqliteTenantRepo ...   │  │  PgTenantRepo ...                │
│  SqliteMigrationRunner  │  │  PgMigrationRunner               │
│  migrations/            │  │  migrations/                     │
│  (SQLite DDL)           │  │  (PostgreSQL DDL)                │
└─────────────────────────┘  └──────────────────────────────────┘
```

## Crate 清单

| Crate | 职责 | 数据库无关 |
|-------|------|-----------|
| `dts-backend` | Trait 定义 + Domain Types + BackendError | ✅ 无任何 DB 依赖 |
| `dts-backend-rdbms` | sqlx 错误映射 + MigrationRunner trait + 密码工具 | 仅依赖 sqlx-core（无 driver） |
| `dts-backend-sqlite` | SQLite 具体实现 | sqlx sqlite driver |
| `dts-backend-postgres` | PostgreSQL 具体实现 | sqlx postgres driver |
| `dts-server` | 应用层，通过 feature flags 选择后端 | 通过 optional deps |

## Feature Flags 设计

```toml
# crates/dts-server/Cargo.toml
[features]
default = ["sqlite"]                    # 开发默认
sqlite = ["dts-backend-sqlite"]         # 编译 SQLite 后端
postgres = ["dts-backend-postgres"]     # 编译 PostgreSQL 后端
```

| 场景 | 编译命令 | 包含后端 |
|------|---------|----------|
| 本地开发 | `cargo build` | 仅 SQLite |
| 本地开发 + PG | `cargo build --features postgres` | SQLite + PostgreSQL |
| 生产部署 | `cargo build --no-default-features --features postgres` | 仅 PostgreSQL |
| CI 测试 | `cargo test` | 仅 SQLite（内存模式） |

## 运行时选择

通过环境变量 `DATABASE_BACKEND` 在运行时选择：

```bash
# 开发默认（SQLite 文件数据库）
DATABASE_BACKEND=sqlite
DATABASE_URL=sqlite://data/dts.db?mode=rwc

# 内存 SQLite（测试用，进程退出后数据消失）
DATABASE_BACKEND=sqlite
DATABASE_URL=sqlite::memory:

# 生产 PostgreSQL
DATABASE_BACKEND=postgres
DATABASE_URL=postgres://user:pass@host:5432/dts
```

派发逻辑在 `dts-server/src/db/mod.rs`：

```rust
pub async fn init_backend(config: &AppConfig) -> anyhow::Result<BackendContext> {
    match config.database_backend.as_str() {
        #[cfg(feature = "sqlite")]
        "sqlite" => init_sqlite(&config.database_url).await,
        #[cfg(feature = "postgres")]
        "postgres" => init_postgres(&config.database_url).await,
        other => anyhow::bail!("不支持的后端: {other}。可用后端: {}", available_backends()),
    }
}
```

## BackendContext 设计

```rust
pub struct BackendContext {
    pub pool: Arc<dyn BackendPool>,           // trait object，后端无关
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub datasource_repo: Arc<dyn DatasourceRepo>,
    pub task_repo: Arc<dyn TaskRepo>,
    pub alert_repo: Arc<dyn AlertRepo>,
    pub monitoring_repo: Arc<dyn MonitoringRepo>,
    pub seed_repo: Arc<dyn SeedRepo>,
}
```

所有字段均为 trait object，应用代码（handler/service）完全不感知底层数据库类型。

## Repository Trait 清单

| Trait | 方法数 | 说明 |
|-------|--------|------|
| `TenantRepo` | 5 | 租户 CRUD + 计数 |
| `UserRepo` | 4 | 用户 CRUD + 角色分配 |
| `RoleRepo` | 4 | 角色 + 权限管理 + 默认角色创建 |
| `DatasourceRepo` | 7 | 数据源 CRUD + 健康统计 |
| `TaskRepo` | 7 | 同步任务 CRUD + 状态更新 + 运行历史 |
| `AlertRepo` | 3 | 告警规则 CRUD |
| `MonitoringRepo` | 1 | 仪表盘聚合查询 |
| `SeedRepo` | 2 | 种子数据检测 + 写入 |

## 新增后端指南

要添加新后端（如 MySQL），只需：

1. 创建 `crates/dts-backend-mysql/`
2. 实现 `BackendPool` trait（`MySqlBackendPool`）
3. 实现 `MigrationRunner` trait（`MySqlMigrationRunner`）
4. 实现 8 个 `*Repo` trait（每种一个 `MySql*Repo` 结构体）
5. 创建 `migrations/` 目录，编写 MySQL DDL
6. 在 `dts-server/Cargo.toml` 添加 `mysql` feature flag
7. 在 `db/mod.rs` 添加 `init_mysql()` + dispatch 分支

## 迁移策略

每个后端独立管理自己的 DDL 迁移：

```
crates/dts-backend-sqlite/migrations/
└── 001_initial_schema.sql    ← SQLite DDL

crates/dts-backend-postgres/migrations/
└── 001_initial_schema.sql    ← PostgreSQL DDL
```

迁移在 `init_*()` 函数中自动执行，种子数据通过 `SeedRepo` trait 统一插入。

## DDL 差异映射

| PostgreSQL | SQLite | 说明 |
|-----------|--------|------|
| `TIMESTAMPTZ` | `TEXT` | SQLite 无原生时间类型，用 ISO 8601 文本 |
| `BOOLEAN` | `INTEGER NOT NULL DEFAULT 0` | SQLite bool 用 0/1 整数模拟 |
| `BIGINT` | `INTEGER` | SQLite 动态类型，无需区分 |
| `DOUBLE PRECISION` | `REAL` | 浮点数 |
| `DEFAULT NOW()` | `DEFAULT (datetime('now'))` | 默认时间函数不同 |
| `ON DELETE CASCADE` | 相同（需 `PRAGMA foreign_keys = ON`） | SQLite 默认关闭外键 |
| `"user"` 引用保留字 | 相同（SQLite 也支持双引号） | 兼容写法 |

## 配置参考

| 环境变量 | 默认值 | 说明 |
|---------|--------|------|
| `DATABASE_BACKEND` | `sqlite` | 后端类型：`sqlite` \| `postgres` |
| `DATABASE_URL` | `sqlite://data/dts.db?mode=rwc` | 连接串（SQLite 文件路径或 PG URL） |
| `JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必须修改） |
| `JWT_EXPIRY_HOURS` | `24` | Token 过期时间（小时） |
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `8080` | 监听端口 |
| `DATA_DIR` | `./data` | 数据目录 |

## 测试策略

| 测试类型 | 数据库 | 说明 |
|---------|--------|------|
| 单元测试 | 无 | 纯逻辑测试，不需要数据库 |
| 集成测试（默认） | SQLite `:memory:` | 零配置，CI 友好 |
| 集成测试（PG 回归） | PostgreSQL | `--features postgres` + 真实 PG 实例 |
