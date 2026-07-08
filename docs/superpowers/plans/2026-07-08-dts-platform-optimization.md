# DTS Platform 工程优化 & ape-dts 引擎集成 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Phase 1 — 补齐 SQLite 后端 + Service 层 + 工程质量提升至 85% 覆盖率；Phase 2 — EngineService trait 抽象 + ape-dts 引擎集成

**Architecture:** 分层 trait 抽象模式：`dts-core`(共享类型) → `dts-backend`(trait) → `dts-backend-sqlite`/`dts-backend-postgres`(实现) → `dts-server`(应用)。引擎侧：`dts-engine`(EngineService trait) → `dts-engine-ape`(ape-dts 实现) → `dts-server`(scheduler)

**Tech Stack:** Rust (actix-web 4.x, sqlx 0.8, tokio), SQLite (dev) + PostgreSQL (prod), tracing (日志), cargo-llvm-cov (覆盖率)

**Design Doc:** `docs/superpowers/specs/2026-07-08-dts-platform-optimization-design.md`

---

## Phase 1 — 工程优化

### Task 1: Create `dts-core` crate — shared types

**Files:**
- Create: `crates/dts-core/Cargo.toml`
- Create: `crates/dts-core/src/lib.rs`
- Create: `crates/dts-core/src/types.rs`

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "dts-core"
version = "0.1.0"
edition = "2021"
description = "DTS Platform 共享基础类型"

[dependencies]
serde = { workspace = true }
chrono = { workspace = true }
```

- [ ] **Step 2: Move types from `dts-backend` to `dts-core`**

```rust
// crates/dts-core/src/types.rs
use serde::{Deserialize, Serialize};

// 租户
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateTenantParams {
    pub id: String,
    pub name: String,
    pub slug: String,
}

// 用户
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CreateUserParams {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
}

// 角色 & 权限
#[derive(Debug, Clone)]
pub struct CreateRoleParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub is_system: bool,
    pub permissions: Vec<String>,
}

// 数据源
#[derive(Debug, Clone)]
pub struct CreateDatasourceParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub encrypted_password: Option<String>,
    pub database_name: Option<String>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceInfo {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub database_name: Option<String>,
    pub health_status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasourceDetail {
    pub id: String,
    pub name: String,
    pub db_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub password_masked: Option<String>,
    pub database_name: Option<String>,
    pub extra_params: Option<String>,
    pub health_status: String,
    pub last_test_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// 同步任务
#[derive(Debug, Clone)]
pub struct CreateTaskParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub config_json: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub cron_expr: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub source_name: String,
    pub source_type: String,
    pub target_name: String,
    pub target_type: String,
    pub cron_expr: Option<String>,
    pub status: String,
    pub config_json: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub recent_runs: Vec<TaskRunInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunInfo {
    pub id: String,
    pub status: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub rows_synced: i64,
    pub bytes_synced: i64,
    pub total_rows: Option<i64>,
    pub total_bytes: Option<i64>,
    pub error_message: Option<String>,
}

// 告警
#[derive(Debug, Clone)]
pub struct CreateAlertParams {
    pub id: String,
    pub tenant_id: String,
    pub task_id: Option<String>,
    pub name: String,
    pub metric_type: String,
    pub operator: String,
    pub threshold: f64,
    pub notify_channels: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertInfo {
    pub id: String,
    pub name: String,
    pub metric_type: String,
    pub operator: String,
    pub threshold: f64,
    pub enabled: bool,
    pub notify_channels: String,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    pub created_at: String,
}

// 监控
#[derive(Debug, Clone, Serialize)]
pub struct MonitoringOverview {
    pub total_tasks: i64,
    pub running_tasks: i64,
    pub failed_tasks: i64,
    pub total_datasources: i64,
    pub healthy_datasources: i64,
    pub today_synced_rows: i64,
    pub today_synced_bytes: i64,
    pub success_rate: f64,
    pub recent_runs: Vec<TaskRunInfo>,
}
```

```rust
// crates/dts-core/src/lib.rs
pub mod types;

pub use types::*;
```

- [ ] **Step 3: Add `dts-core` to workspace members**

```bash
# Edit Cargo.toml (workspace root)
# Change: members = ["crates/dts-server", "crates/dts-backend", "crates/dts-backend-rdbms", "crates/dts-backend-postgres"]
# To:     members = ["crates/dts-server", "crates/dts-backend", "crates/dts-backend-rdbms", "crates/dts-backend-postgres", "crates/dts-core"]
```

- [ ] **Step 4: Verify the crate compiles**

Run: `cargo build -p dts-core`
Expected: Compilation success

- [ ] **Step 5: Update `dts-backend/Cargo.toml` to depend on `dts-core`**

```toml
[dependencies]
dts-core = { path = "../dts-core" }
serde = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
async-trait = { workspace = true }
```

- [ ] **Step 6: Update `dts-backend/src/lib.rs` to re-export from dts-core**

```rust
// crates/dts-backend/src/lib.rs
pub mod error;
pub mod pool;
pub mod repo;

// Re-export types from dts-core (backward compatible)
pub use dts_core::*;

pub use error::BackendError;
pub use pool::{BackendPool, BackendPoolFactory};
pub use repo::{
    AlertRepo, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo, TenantRepo, UserRepo,
};
```

- [ ] **Step 7: Remove `crates/dts-backend/src/types.rs`** (content moved to dts-core)

- [ ] **Step 8: Verify full workspace compiles**

Run: `cargo build`
Expected: Compilation success for all crates

- [ ] **Step 9: Commit**

```bash
git add crates/dts-core/ crates/dts-backend/ Cargo.toml
git commit -m "refactor: extract shared types into dts-core crate"
```

---

### Task 2: Create `dts-backend-sqlite` crate skeleton

**Files:**
- Create: `crates/dts-backend-sqlite/Cargo.toml`
- Create: `crates/dts-backend-sqlite/src/lib.rs`
- Create: `crates/dts-backend-sqlite/src/pool.rs`
- Create: `crates/dts-backend-sqlite/src/migrate.rs`
- Create: `crates/dts-backend-sqlite/src/repo/mod.rs`
- Create: `crates/dts-backend-sqlite/migrations/001_initial_schema.sql`

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "dts-backend-sqlite"
version = "0.1.0"
edition = "2021"
description = "DTS Platform SQLite 存储后端"

[lints]
workspace = true

[dependencies]
dts-backend = { path = "../dts-backend" }
dts-backend-rdbms = { path = "../dts-backend-rdbms" }
dts-core = { path = "../dts-core" }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
async-trait = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true, features = ["v4"] }
log = { workspace = true }
argon2 = "0.5"
```

- [ ] **Step 2: Write `lib.rs`**

```rust
// crates/dts-backend-sqlite/src/lib.rs
//! DTS Backend SQLite — SQLite 协议存储后端
//!
//! 基于 sqlx 实现，用于本地开发和测试。
//! 使用 sqlite::memory: 模式可实现零配置测试。

pub mod pool;
pub mod repo;

mod migrate;

pub use migrate::SqliteMigrationRunner;
pub use pool::{SqliteBackendPool, create_pool};
pub use repo::{
    SqliteAlertRepo, SqliteDatasourceRepo, SqliteMonitoringRepo, SqliteRoleRepo, SqliteSeedRepo,
    SqliteTaskRepo, SqliteTenantRepo, SqliteUserRepo,
};
```

- [ ] **Step 3: Write `pool.rs`**

```rust
// crates/dts-backend-sqlite/src/pool.rs
use async_trait::async_trait;
use dts_backend::{BackendError, BackendPool};
use dts_backend_rdbms::map_sqlx_error;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// SQLite 连接池适配器
#[derive(Clone)]
pub struct SqliteBackendPool {
    pool: SqlitePool,
}

impl SqliteBackendPool {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 获取内部 SqlitePool（供 Repository 实现使用）
    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl BackendPool for SqliteBackendPool {
    fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    async fn ping(&self) -> Result<(), BackendError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}

/// 创建 SQLite 连接池
pub async fn create_pool(database_url: &str) -> Result<SqliteBackendPool, BackendError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(map_sqlx_error)?;

    // 启用 WAL 模式和外键约束
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(map_sqlx_error)?;

    log::info!("SQLite 连接池已创建");
    Ok(SqliteBackendPool::new(pool))
}
```

- [ ] **Step 4: Write `migrate.rs`**

```rust
// crates/dts-backend-sqlite/src/migrate.rs
use async_trait::async_trait;
use dts_backend::BackendError;
use dts_backend_rdbms::MigrationRunner;
use sqlx::SqlitePool;

/// SQLite 迁移执行器
pub struct SqliteMigrationRunner {
    pool: SqlitePool,
}

impl SqliteMigrationRunner {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MigrationRunner for SqliteMigrationRunner {
    async fn run_migrations(&self) -> Result<(), BackendError> {
        let sql = include_str!("../migrations/001_initial_schema.sql");
        // SQLite 不支持多语句单次执行，逐条执行
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                continue;
            }
            sqlx::query(trimmed)
                .execute(&self.pool)
                .await
                .map_err(|e| BackendError::Migration(format!("SQLite 迁移失败: {e}")))?;
        }
        log::info!("SQLite 数据库迁移完成");
        Ok(())
    }
}
```

- [ ] **Step 5: Write SQLite DDL migration**

```sql
-- crates/dts-backend-sqlite/migrations/001_initial_schema.sql
CREATE TABLE IF NOT EXISTS tenant (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS "user" (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, username),
    UNIQUE(tenant_id, email)
);

CREATE TABLE IF NOT EXISTS role (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

CREATE TABLE IF NOT EXISTS user_role (
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE IF NOT EXISTS role_permission (
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE IF NOT EXISTS datasource (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    db_type TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT,
    encrypted_password TEXT,
    database_name TEXT,
    extra_params TEXT DEFAULT '{}',
    health_status TEXT NOT NULL DEFAULT 'unknown',
    last_test_at TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sync_task (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    source_id TEXT NOT NULL REFERENCES datasource(id) ON DELETE RESTRICT,
    target_id TEXT NOT NULL REFERENCES datasource(id) ON DELETE RESTRICT,
    cron_expr TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    config_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS task_run (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES sync_task(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'running',
    start_time TEXT NOT NULL DEFAULT (datetime('now')),
    end_time TEXT,
    rows_synced INTEGER NOT NULL DEFAULT 0,
    bytes_synced INTEGER NOT NULL DEFAULT 0,
    total_rows INTEGER DEFAULT 0,
    total_bytes INTEGER DEFAULT 0,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_task_run_task_id ON task_run(task_id, start_time DESC);

CREATE TABLE IF NOT EXISTS alert_rule (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES sync_task(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    metric_type TEXT NOT NULL,
    operator TEXT NOT NULL,
    threshold REAL NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    notify_channels TEXT DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    detail TEXT DEFAULT '{}',
    ip_address TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_log_tenant ON audit_log(tenant_id, created_at DESC)
```

- [ ] **Step 6: Write `repo/mod.rs`**

```rust
// crates/dts-backend-sqlite/src/repo/mod.rs
pub mod alert;
pub mod datasource;
pub mod monitoring;
pub mod role;
pub mod seed;
pub mod task;
pub mod tenant;
pub mod user;

pub use alert::SqliteAlertRepo;
pub use datasource::SqliteDatasourceRepo;
pub use monitoring::SqliteMonitoringRepo;
pub use role::SqliteRoleRepo;
pub use seed::SqliteSeedRepo;
pub use task::SqliteTaskRepo;
pub use tenant::SqliteTenantRepo;
pub use user::SqliteUserRepo;
```

- [ ] **Step 7: Add to workspace members**

```bash
# Edit root Cargo.toml, add "crates/dts-backend-sqlite" to members
```

- [ ] **Step 8: Verify crate compiles (with stubs)**

Run: `cargo build -p dts-backend-sqlite`
Expected: Fails with "not all trait items implemented" (expected — we add repos next)

- [ ] **Step 9: Commit**

```bash
git add crates/dts-backend-sqlite/ Cargo.toml
git commit -m "feat: add dts-backend-sqlite crate skeleton with pool, migrations, DDL"
```

---

### Task 3: Implement SQLite Repository layer — Tenant + User + Role

**Files:**
- Create: `crates/dts-backend-sqlite/src/repo/tenant.rs`
- Create: `crates/dts-backend-sqlite/src/repo/user.rs`
- Create: `crates/dts-backend-sqlite/src/repo/role.rs`

- [ ] **Step 1: Write failing integration tests for SqliteTenantRepo**

```rust
// crates/dts-backend-sqlite/tests/tenant_test.rs
use dts_backend::repo::TenantRepo;
use dts_backend_sqlite::{SqliteTenantRepo, create_pool};
use dts_core::types::CreateTenantParams;
use uuid::Uuid;
use sqlx::SqlitePool;

async fn setup() -> (SqlitePool, SqliteTenantRepo) {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    // Run migrations
    let runner = dts_backend_sqlite::SqliteMigrationRunner::new(pool.clone());
    runner.run_migrations().await.unwrap();
    let repo = SqliteTenantRepo::new(pool.clone());
    (pool, repo)
}

#[tokio::test]
async fn create_and_find_tenant() {
    let (_pool, repo) = setup().await;
    let id = Uuid::new_v4().to_string();
    let params = CreateTenantParams {
        id: id.clone(),
        name: "Test Tenant".into(),
        slug: "test".into(),
    };
    let result = repo.create(&params).await.unwrap();
    assert_eq!(result, id);

    let found = repo.find_by_slug("test").await.unwrap().unwrap();
    assert_eq!(found.name, "Test Tenant");
    assert_eq!(found.status, "active");
}

#[tokio::test]
async fn list_all_tenants() {
    let (_pool, repo) = setup().await;
    let id1 = Uuid::new_v4().to_string();
    let id2 = Uuid::new_v4().to_string();
    repo.create(&CreateTenantParams { id: id1, name: "A".into(), slug: "a".into() }).await.unwrap();
    repo.create(&CreateTenantParams { id: id2, name: "B".into(), slug: "b".into() }).await.unwrap();

    let all = repo.list_all().await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn tenant_not_found() {
    let (_pool, repo) = setup().await;
    let result = repo.find_by_slug("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn count_tenants() {
    let (_pool, repo) = setup().await;
    assert_eq!(repo.count().await.unwrap(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p dts-backend-sqlite --test tenant_test`
Expected: FAIL — module not found

- [ ] **Step 3: Implement SqliteTenantRepo**

```rust
// crates/dts-backend-sqlite/src/repo/tenant.rs
use async_trait::async_trait;
use dts_backend::{BackendError, repo::TenantRepo};
use dts_backend_rdbms::map_sqlx_error;
use dts_core::types::*;
use sqlx::SqlitePool;
use sqlx::Row;

pub struct SqliteTenantRepo {
    pool: SqlitePool,
}

impl SqliteTenantRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_info(r: &sqlx::sqlite::SqliteRow) -> TenantInfo {
        let created_at_str: String = r.get(4);
        TenantInfo {
            id: r.get(0),
            name: r.get(1),
            slug: r.get(2),
            status: r.get(3),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .unwrap_or_default()
                .with_timezone(&chrono::Utc),
        }
    }
}

#[async_trait]
impl TenantRepo for SqliteTenantRepo {
    async fn create(&self, params: &CreateTenantParams) -> Result<String, BackendError> {
        sqlx::query("INSERT INTO tenant (id, name, slug) VALUES (?1, ?2, ?3)")
            .bind(&params.id)
            .bind(&params.name)
            .bind(&params.slug)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row = sqlx::query(
            "SELECT id, name, slug, status, created_at FROM tenant WHERE slug = ?1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<TenantInfo>, BackendError> {
        let row = sqlx::query(
            "SELECT id, name, slug, status, created_at FROM tenant WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_info))
    }

    async fn list_all(&self) -> Result<Vec<TenantInfo>, BackendError> {
        let rows = sqlx::query(
            "SELECT id, name, slug, status, created_at FROM tenant ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.iter().map(Self::row_to_info).collect())
    }

    async fn count(&self) -> Result<i64, BackendError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM tenant")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p dts-backend-sqlite --test tenant_test`
Expected: All 4 tests PASS

- [ ] **Step 5: Write + implement SqliteUserRepo**

Same pattern — write integration test first, then implementation. Key queries use `?1` bind syntax.

```rust
// crates/dts-backend-sqlite/src/repo/user.rs
use async_trait::async_trait;
use dts_backend::{BackendError, repo::UserRepo};
use dts_backend_rdbms::map_sqlx_error;
use dts_core::types::*;
use sqlx::SqlitePool;
use sqlx::Row;

pub struct SqliteUserRepo {
    pool: SqlitePool,
}

impl SqliteUserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_record(r: &sqlx::sqlite::SqliteRow) -> UserRecord {
        UserRecord {
            id: r.get(0),
            tenant_id: r.get(1),
            username: r.get(2),
            email: r.get(3),
            password_hash: r.get(4),
            display_name: r.get(5),
            status: r.get(6),
        }
    }
}

#[async_trait]
impl UserRepo for SqliteUserRepo {
    async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO \"user\" (id, tenant_id, username, email, password_hash, display_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.username)
        .bind(&params.email)
        .bind(&params.password_hash)
        .bind(&params.display_name)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(params.id.clone())
    }

    async fn find_by_username(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, BackendError> {
        let row = sqlx::query(
            "SELECT id, tenant_id, username, email, password_hash, display_name, status
             FROM \"user\" WHERE tenant_id = ?1 AND username = ?2",
        )
        .bind(tenant_id)
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_record))
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<UserRecord>, BackendError> {
        let row = sqlx::query(
            "SELECT id, tenant_id, username, email, password_hash, display_name, status
             FROM \"user\" WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.as_ref().map(Self::row_to_record))
    }

    async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError> {
        sqlx::query("INSERT OR IGNORE INTO user_role (user_id, role_id) VALUES (?1, ?2)")
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}
```

- [ ] **Step 6: Write + implement SqliteRoleRepo**

```rust
// crates/dts-backend-sqlite/src/repo/role.rs
use async_trait::async_trait;
use dts_backend::{BackendError, repo::RoleRepo};
use dts_backend_rdbms::map_sqlx_error;
use dts_core::types::*;
use sqlx::SqlitePool;
use sqlx::Row;

pub struct SqliteRoleRepo {
    pool: SqlitePool,
}

impl SqliteRoleRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoleRepo for SqliteRoleRepo {
    async fn create(&self, params: &CreateRoleParams) -> Result<String, BackendError> {
        sqlx::query(
            "INSERT INTO role (id, tenant_id, name, description, is_system)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(params.is_system as i32)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Insert permissions
        for perm in &params.permissions {
            sqlx::query("INSERT OR IGNORE INTO role_permission (role_id, permission) VALUES (?1, ?2)")
                .bind(&params.id)
                .bind(perm)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        }
        Ok(params.id.clone())
    }

    async fn find_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<(String, Vec<String>)>, BackendError> {
        let role_row = sqlx::query(
            "SELECT id FROM role WHERE tenant_id = ?1 AND name = ?2",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        match role_row {
            Some(r) => {
                let role_id: String = r.get(0);
                let perms: Vec<(String,)> = sqlx::query_as(
                    "SELECT permission FROM role_permission WHERE role_id = ?1",
                )
                .bind(&role_id)
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
                let permissions: Vec<String> = perms.into_iter().map(|(p,)| p).collect();
                Ok(Some((role_id, permissions)))
            }
            None => Ok(None),
        }
    }

    async fn get_user_permissions(
        &self,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<String>, BackendError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT rp.permission
             FROM user_role ur
             JOIN role r ON ur.role_id = r.id
             JOIN role_permission rp ON r.id = rp.role_id
             WHERE ur.user_id = ?1 AND r.tenant_id = ?2",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(rows.into_iter().map(|(p,)| p).collect())
    }

    async fn create_default_roles(
        &self,
        tenant_id: &str,
    ) -> Result<dts_backend::repo::DefaultRoles, BackendError> {
        // tenant_admin
        let admin_id = uuid::Uuid::new_v4().to_string();
        self.create(&CreateRoleParams {
            id: admin_id.clone(),
            tenant_id: tenant_id.into(),
            name: "tenant_admin".into(),
            description: "租户管理员".into(),
            is_system: true,
            permissions: vec![
                "tenant:manage".into(), "user:manage".into(), "datasource:*".into(),
                "task:*".into(), "alert:*".into(), "monitoring:*".into(),
            ],
        }).await?;

        // developer
        let dev_id = uuid::Uuid::new_v4().to_string();
        self.create(&CreateRoleParams {
            id: dev_id.clone(),
            tenant_id: tenant_id.into(),
            name: "developer".into(),
            description: "开发者".into(),
            is_system: true,
            permissions: vec![
                "datasource:*".into(), "task:*".into(), "monitoring:*".into(),
            ],
        }).await?;

        // viewer
        let viewer_id = uuid::Uuid::new_v4().to_string();
        self.create(&CreateRoleParams {
            id: viewer_id.clone(),
            tenant_id: tenant_id.into(),
            name: "viewer".into(),
            description: "只读用户".into(),
            is_system: true,
            permissions: vec!["monitoring:read".into()],
        }).await?;

        Ok(dts_backend::repo::DefaultRoles {
            admin_role_id: admin_id,
            developer_role_id: dev_id,
            viewer_role_id: viewer_id,
        })
    }
}
```

- [ ] **Step 7: Verify all 3 repos compile + tests pass**

Run: `cargo test -p dts-backend-sqlite`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/dts-backend-sqlite/
git commit -m "feat: implement SqliteTenantRepo, SqliteUserRepo, SqliteRoleRepo with tests"
```

---

### Task 4: Implement remaining SQLite Repos — Datasource + Task + Alert + Monitoring + Seed

**Files:**
- Create: `crates/dts-backend-sqlite/src/repo/datasource.rs`
- Create: `crates/dts-backend-sqlite/src/repo/task.rs`
- Create: `crates/dts-backend-sqlite/src/repo/alert.rs`
- Create: `crates/dts-backend-sqlite/src/repo/monitoring.rs`
- Create: `crates/dts-backend-sqlite/src/repo/seed.rs`

These follow the exact same pattern as Task 3 but for the remaining 5 repositories. Each SQLite repo mirrors the corresponding `dts-backend-postgres/src/repo/*.rs` file, substituting:

| PostgreSQL | SQLite |
|-----------|--------|
| `$1, $2, ...` bind params | `?1, ?2, ...` bind params |
| `PgPool` | `SqlitePool` |
| `sqlx::postgres::PgRow` | `sqlx::sqlite::SqliteRow` |
| `sqlx::query(...)` | `sqlx::query(...)` (same macro) |

- [ ] **Step 1: Implement `SqliteDatasourceRepo`** — follows `PgDatasourceRepo` pattern, maps `port` from `INTEGER` (SQLite) to `i64`
- [ ] **Step 2: Implement `SqliteTaskRepo`** — note SQLite has no `ARRAY_AGG`; replace the JOIN approach in `find_detail_by_id` with two separate queries
- [ ] **Step 3: Implement `SqliteAlertRepo`** — straightforward CRUD mapping
- [ ] **Step 4: Implement `SqliteMonitoringRepo`** — aggregate queries compatible with SQLite
- [ ] **Step 5: Implement `SqliteSeedRepo`** — uses `hash_password_sync` (same as PG seed), creates default tenant/admin/dev/viewer
- [ ] **Step 6: Write integration tests for all 5 repos** — SQLite `:memory:` mode
- [ ] **Step 7: Run full test suite**

Run: `cargo test -p dts-backend-sqlite`
Expected: All tests PASS (8 repos × ~4 tests each ≈ 32 tests)

- [ ] **Step 8: Commit**

```bash
git add crates/dts-backend-sqlite/src/repo/
git commit -m "feat: implement remaining SQLite repos (datasource, task, alert, monitoring, seed)"
```

---

### Task 5: Add feature flags + runtime backend dispatch in `dts-server`

**Files:**
- Modify: `crates/dts-server/Cargo.toml`
- Modify: `crates/dts-server/src/db/mod.rs`
- Modify: `crates/dts-server/src/main.rs`

- [ ] **Step 1: Update `dts-server/Cargo.toml`** with feature flags

```toml
# Change hard deps to optional:
dts-backend-rdbms = { path = "../dts-backend-rdbms" }
dts-backend-sqlite = { path = "../dts-backend-sqlite", optional = true }
dts-backend-postgres = { path = "../dts-backend-postgres", optional = true }

[features]
default = ["sqlite"]
sqlite = ["dts-backend-sqlite"]
postgres = ["dts-backend-postgres"]
```

- [ ] **Step 2: Add to `release` profile** in root `Cargo.toml`

```toml
[profile.release]
# Same as existing, add:
# (Default features already sqlite; --features postgres adds PG)
```

- [ ] **Step 3: Rewrite `db/mod.rs`** for runtime dispatch

```rust
// crates/dts-server/src/db/mod.rs
use std::sync::Arc;
use dts_backend::repo::{
    AlertRepo, DatasourceRepo, MonitoringRepo, RoleRepo, SeedRepo, TaskRepo, TenantRepo, UserRepo,
};
use dts_backend::BackendPool;
use dts_backend_rdbms::MigrationRunner;

/// 后端上下文 — 持有连接池和所有 Repository (全部 trait object)
pub struct BackendContext {
    pub pool: Arc<dyn BackendPool>,
    pub tenant_repo: Arc<dyn TenantRepo>,
    pub user_repo: Arc<dyn UserRepo>,
    pub role_repo: Arc<dyn RoleRepo>,
    pub datasource_repo: Arc<dyn DatasourceRepo>,
    pub task_repo: Arc<dyn TaskRepo>,
    pub alert_repo: Arc<dyn AlertRepo>,
    pub monitoring_repo: Arc<dyn MonitoringRepo>,
    pub seed_repo: Arc<dyn SeedRepo>,
}

/// 运行时初始化存储后端
pub async fn init_backend(database_url: &str, backend_type: &str) -> anyhow::Result<BackendContext> {
    match backend_type {
        #[cfg(feature = "sqlite")]
        "sqlite" => init_sqlite(database_url).await,
        #[cfg(feature = "postgres")]
        "postgres" => init_postgres(database_url).await,
        other => anyhow::anyhow!("不支持的后端类型: {other}。可用: sqlite, postgres"),
    }
}

#[cfg(feature = "sqlite")]
async fn init_sqlite(database_url: &str) -> anyhow::Result<BackendContext> {
    use dts_backend_sqlite::{
        SqliteAlertRepo, SqliteDatasourceRepo, SqliteMigrationRunner, SqliteMonitoringRepo,
        SqliteRoleRepo, SqliteSeedRepo, SqliteTaskRepo, SqliteTenantRepo, SqliteUserRepo,
    };

    let pool = dts_backend_sqlite::create_pool(database_url).await?;
    let inner = pool.inner().clone();

    let runner = SqliteMigrationRunner::new(inner.clone());
    runner.run_migrations().await?;

    let seed_repo = Arc::new(SqliteSeedRepo::new(inner.clone()));
    if !seed_repo.has_data().await? {
        seed_repo.seed().await?;
    } else {
        tracing::info!("数据库已有数据，跳过种子数据初始化");
    }

    let pool: Arc<dyn BackendPool> = Arc::new(pool);

    Ok(BackendContext {
        pool: pool.clone(),
        tenant_repo: Arc::new(SqliteTenantRepo::new(inner.clone())),
        user_repo: Arc::new(SqliteUserRepo::new(inner.clone())),
        role_repo: Arc::new(SqliteRoleRepo::new(inner.clone())),
        datasource_repo: Arc::new(SqliteDatasourceRepo::new(inner.clone())),
        task_repo: Arc::new(SqliteTaskRepo::new(inner.clone())),
        alert_repo: Arc::new(SqliteAlertRepo::new(inner.clone())),
        monitoring_repo: Arc::new(SqliteMonitoringRepo::new(inner.clone())),
        seed_repo,
    })
}

#[cfg(feature = "postgres")]
async fn init_postgres(database_url: &str) -> anyhow::Result<BackendContext> {
    use dts_backend_postgres::{
        PgAlertRepo, PgDatasourceRepo, PgMigrationRunner, PgMonitoringRepo,
        PgRoleRepo, PgSeedRepo, PgTaskRepo, PgTenantRepo, PgUserRepo,
    };

    let pool = dts_backend_postgres::create_pool(database_url).await?;
    let inner = pool.inner().clone();

    let runner = PgMigrationRunner::new(inner.clone());
    runner.run_migrations().await?;

    let seed_repo = Arc::new(PgSeedRepo::new(inner.clone()));
    if !seed_repo.has_data().await? {
        seed_repo.seed().await?;
    } else {
        tracing::info!("数据库已有数据，跳过种子数据初始化");
    }

    let pool: Arc<dyn BackendPool> = Arc::new(pool);

    Ok(BackendContext {
        pool: pool.clone(),
        tenant_repo: Arc::new(PgTenantRepo::new(inner.clone())),
        user_repo: Arc::new(PgUserRepo::new(inner.clone())),
        role_repo: Arc::new(PgRoleRepo::new(inner.clone())),
        datasource_repo: Arc::new(PgDatasourceRepo::new(inner.clone())),
        task_repo: Arc::new(PgTaskRepo::new(inner.clone())),
        alert_repo: Arc::new(PgAlertRepo::new(inner.clone())),
        monitoring_repo: Arc::new(PgMonitoringRepo::new(inner.clone())),
        seed_repo,
    })
}
```

- [ ] **Step 4: Update `main.rs`** to use runtime dispatch

```rust
// crates/dts-server/src/main.rs
use dts_server::config::AppConfig;
use dts_server::{app, db};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (replaces env_logger)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,dts_server=debug".into()),
        )
        .init();

    tracing::info!("=== DTS Platform 企业级数据同步管理平台 ===");
    tracing::info!("版本: {}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = AppConfig::from_env()?;
    tracing::info!("服务地址: {}:{}", config.server_host, config.server_port);
    tracing::info!("存储后端: {}", config.database_backend);

    // Initialize backend (runtime dispatch)
    let ctx = db::init_backend(&config.database_url, &config.database_backend).await?;
    ctx.pool.ping().await?;

    // Start web server
    let server = app::create_app(ctx, config.clone()).await?;
    tracing::info!("服务已启动!");
    tracing::info!("健康检查: http://localhost:{}/health", config.server_port);

    server.await?;
    Ok(())
}
```

- [ ] **Step 5: Update `app.rs`** — change pool type to `Arc<dyn BackendPool>`

In `app.rs`, change `web::Data::new(ctx.pool.clone())` — already works because `BackendPool: Clone`. No signature change needed since `BackendContext.pool` is now `Arc<dyn BackendPool>`.

- [ ] **Step 6: Build with default features (SQLite only)**

Run: `cargo build`
Expected: Compilation success

- [ ] **Step 7: Build with PostgreSQL features**

Run: `cargo build --features postgres`
Expected: Compilation success

- [ ] **Step 8: Run all tests**

Run: `cargo test --all`
Expected: All tests PASS

- [ ] **Step 9: Commit**

```bash
git add crates/dts-server/Cargo.toml crates/dts-server/src/db/mod.rs crates/dts-server/src/main.rs crates/dts-server/src/app.rs Cargo.toml
git commit -m "feat: add feature flags + runtime backend dispatch (sqlite/postgres)"
```

---

### Task 6: Create Service layer — UserService + DatasourceService

**Files:**
- Create: `crates/dts-server/src/services/user_svc.rs`
- Create: `crates/dts-server/src/services/datasource_svc.rs`
- Modify: `crates/dts-server/src/services/mod.rs`

- [ ] **Step 1: Write failing unit tests for UserService**

```rust
// crates/dts-server/src/services/user_svc.rs
use std::sync::Arc;
use async_trait::async_trait;
use dts_backend::repo::UserRepo;
use dts_backend::BackendError;
use dts_core::types::{CreateUserParams, UserRecord};

pub struct UserService {
    user_repo: Arc<dyn UserRepo>,
}

impl UserService {
    pub fn new(user_repo: Arc<dyn UserRepo>) -> Self {
        Self { user_repo }
    }

    pub async fn create_user(
        &self,
        tenant_id: &str,
        username: &str,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<UserRecord, BackendError> {
        use argon2::{
            PasswordHasher,
            password_hash::{SaltString, rand_core::OsRng},
        };

        let salt = SaltString::generate(&mut OsRng);
        let hash = argon2::Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| BackendError::Internal(format!("password hashing failed: {e}")))?;

        let id = uuid::Uuid::new_v4().to_string();
        let params = CreateUserParams {
            id: id.clone(),
            tenant_id: tenant_id.into(),
            username: username.into(),
            email: email.into(),
            password_hash: hash.to_string(),
            display_name: display_name.map(String::from),
        };

        self.user_repo.create(&params).await?;
        self.user_repo
            .find_by_id(&id)
            .await
            .map(|u| u.expect("just created user must exist"))
    }

    pub async fn find_by_username(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<Option<UserRecord>, BackendError> {
        self.user_repo.find_by_username(tenant_id, username).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dts_backend::repo::UserRepo;
    use mockall::predicate::*;
    use mockall::mock;

    mock! {
        pub UserRepoMock {}
        #[async_trait]
        impl UserRepo for UserRepoMock {
            async fn create(&self, params: &CreateUserParams) -> Result<String, BackendError>;
            async fn find_by_username(&self, tenant_id: &str, username: &str) -> Result<Option<UserRecord>, BackendError>;
            async fn find_by_id(&self, id: &str) -> Result<Option<UserRecord>, BackendError>;
            async fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), BackendError>;
        }
    }

    #[tokio::test]
    async fn create_user_hashes_password_and_returns_record() {
        let mut mock = MockUserRepoMock::new();

        // Expect create to be called
        mock.expect_create()
            .times(1)
            .returning(|p| Ok(p.id.clone()));

        // Expect find_by_id to return the created user
        mock.expect_find_by_id()
            .times(1)
            .returning(|id| {
                Ok(Some(UserRecord {
                    id: id.into(),
                    tenant_id: "tenant-1".into(),
                    username: "testuser".into(),
                    email: "test@example.com".into(),
                    password_hash: "hashed".into(),
                    display_name: Some("Test User".into()),
                    status: "active".into(),
                }))
            });

        let svc = UserService::new(Arc::new(mock));
        let user = svc
            .create_user("tenant-1", "testuser", "test@example.com", "secret123", Some("Test User"))
            .await
            .unwrap();

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.display_name, Some("Test User".into()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail/pass**

Run: `cargo test -p dts-server --lib -- services::user_svc`
Expected: PASS (tests with mocks)

- [ ] **Step 3: Implement DatasourceService**

```rust
// crates/dts-server/src/services/datasource_svc.rs
use std::sync::Arc;
use dts_backend::repo::DatasourceRepo;
use dts_backend::BackendError;
use dts_core::types::{CreateDatasourceParams, DatasourceInfo};

pub struct DatasourceService {
    datasource_repo: Arc<dyn DatasourceRepo>,
}

impl DatasourceService {
    pub fn new(datasource_repo: Arc<dyn DatasourceRepo>) -> Self {
        Self { datasource_repo }
    }

    pub async fn create(
        &self,
        tenant_id: &str,
        name: &str,
        db_type: &str,
        host: &str,
        port: i64,
        username: Option<&str>,
        password: Option<&str>,
        database_name: Option<&str>,
        created_by: &str,
    ) -> Result<String, BackendError> {
        let id = uuid::Uuid::new_v4().to_string();
        let encrypted = password.map(|p| encrypt_password(p));
        let params = CreateDatasourceParams {
            id: id.clone(),
            tenant_id: tenant_id.into(),
            name: name.into(),
            db_type: db_type.into(),
            host: host.into(),
            port,
            username: username.map(String::from),
            encrypted_password: encrypted,
            database_name: database_name.map(String::from),
            created_by: created_by.into(),
        };
        self.datasource_repo.create(&params).await
    }

    pub async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<DatasourceInfo>, BackendError> {
        self.datasource_repo.list_by_tenant(tenant_id).await
    }

    pub async fn exists(&self, id: &str, tenant_id: &str) -> Result<bool, BackendError> {
        self.datasource_repo.exists(id, tenant_id).await
    }
}

/// Simple password encryption placeholder (AES-GCM in production)
fn encrypt_password(plain: &str) -> String {
    format!("enc:{}", plain) // TODO: replace with AES-256-GCM in Phase 2
}
```

- [ ] **Step 4: Update `services/mod.rs`**

```rust
pub mod auth_svc;
pub mod datasource_svc;
pub mod tenant_svc;
pub mod user_svc;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p dts-server`
Expected: Compilation success

- [ ] **Step 6: Commit**

```bash
git add crates/dts-server/src/services/
git commit -m "feat: add UserService and DatasourceService with unit tests"
```

---

### Task 7: Create remaining Services — TaskService + AlertService + MonitoringService

**Files:**
- Create: `crates/dts-server/src/services/task_svc.rs`
- Create: `crates/dts-server/src/services/alert_svc.rs`
- Create: `crates/dts-server/src/services/monitoring_svc.rs`

Same pattern as Task 6. Each service wraps the corresponding Repository trait, adds business logic (validation, state machine, encryption), and is tested with mockall mocks.

- [ ] **Step 1: Implement TaskService** — wraps `Arc<dyn TaskRepo>` + `Arc<dyn DatasourceRepo>`, validates source/target exist on create, enforces status transitions (draft→running→completed/failed, running→paused), config serialization
- [ ] **Step 2: Implement AlertService** — wraps `Arc<dyn AlertRepo>`, validates metric_type/operator enum values, toggle enable/disable
- [ ] **Step 3: Implement MonitoringService** — wraps `Arc<dyn MonitoringRepo>` + `Arc<dyn TaskRepo>`, computes success_rate
- [ ] **Step 4: Write unit tests for all 3 services** with mocks
- [ ] **Step 5: Run tests** — `cargo test -p dts-server --lib`
- [ ] **Step 6: Commit**

---

### Task 8: Refactor handlers to use Service layer

**Files:**
- Modify: `crates/dts-server/src/api/tasks.rs`
- Modify: `crates/dts-server/src/api/datasources.rs`
- Modify: `crates/dts-server/src/api/alerts.rs`
- Modify: `crates/dts-server/src/api/monitoring.rs`
- Modify: `crates/dts-server/src/app.rs`

Each handler changes from injecting `Arc<dyn *Repo>` directly to injecting the corresponding Service. The handler becomes a thin pass-through.

- [ ] **Step 1: Refactor `api/tasks.rs`** — inject `TaskService` + `DatasourceService`, delegate business logic

```rust
// Before:
// pub async fn list(task_repo: web::Data<Arc<dyn TaskRepo>>, ...)
// After:
// pub async fn list(task_svc: web::Data<TaskService>, ...)

// Before:
// task_repo.list_by_tenant(&claims.tenant_id).await?
// After:
// task_svc.list_by_tenant(&claims.tenant_id).await?

// Before (create):
// Validate ds existence inline → call task_repo.create()
// After (create):
// task_svc.create(tenant_id, name, ...).await?  // validation inside service
```

- [ ] **Step 2: Refactor `api/datasources.rs`** — inject `DatasourceService`
- [ ] **Step 3: Refactor `api/alerts.rs`** — inject `AlertService`
- [ ] **Step 4: Refactor `api/monitoring.rs`** — inject `MonitoringService` + `TaskService`
- [ ] **Step 5: Update `app.rs`** — construct services and register as app data

```rust
// In create_app():
let task_svc = TaskService::new(ctx.task_repo.clone(), ctx.datasource_repo.clone());
let datasource_svc = DatasourceService::new(ctx.datasource_repo.clone());
let alert_svc = AlertService::new(ctx.alert_repo.clone());
let monitoring_svc = MonitoringService::new(ctx.monitoring_repo.clone(), ctx.task_repo.clone());

// Register:
.app_data(web::Data::new(task_svc))
.app_data(web::Data::new(datasource_svc))
.app_data(web::Data::new(alert_svc))
.app_data(web::Data::new(monitoring_svc))
```

- [ ] **Step 6: Run all tests** — `cargo test --all`
- [ ] **Step 7: Commit**

```bash
git add crates/dts-server/src/api/ crates/dts-server/src/app.rs
git commit -m "refactor: handlers delegate to Service layer (thin handlers)"
```

---

### Task 9: Split AuthService — extract JWT logic + `log` → `tracing` migration

**Files:**
- Create: `crates/dts-server/src/services/token.rs`
- Modify: `crates/dts-server/src/services/auth_svc.rs`
- Modify: All `.rs` files using `log::` macros

- [ ] **Step 1: Extract `token.rs`** from auth_svc

```rust
// crates/dts-server/src/services/token.rs
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub tenant_id: String,
    pub username: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn create_token(claims: &Claims, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
```

- [ ] **Step 2: Update `auth_svc.rs`** — use `token` module, simplify

AuthService now delegates token ops to `token::create_token` / `token::validate_token`. Core login flow: validate credentials → create token → return.

- [ ] **Step 3: Replace `log` with `tracing`** across all crates

```bash
# In Cargo.toml for dts-server, dts-backend-postgres, dts-backend-sqlite:
# Replace: log = { workspace = true }, env_logger = { workspace = true }
# Add: tracing = "0.1", tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

Global search-replace:
- `log::info!(...)` → `tracing::info!(...)`
- `log::error!(...)` → `tracing::error!(...)`
- `log::debug!(...)` → `tracing::debug!(...)`
- `log::warn!(...)` → `tracing::warn!(...)`

- [ ] **Step 4: Verify compilation** — `cargo build --all`
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: extract JWT token module, migrate log → tracing"
```

---

### Task 10: Const extraction + clippy cleanup + config hardening

**Files:**
- Modify: `crates/dts-server/src/config.rs`
- Modify: Various files with hardcoded strings

- [ ] **Step 1: Extract constants in `config.rs`**

```rust
// crates/dts-server/src/config.rs
/// Default values used when environment variables are not set
pub mod defaults {
    pub const DATABASE_URL: &str = "sqlite://data/dts.db?mode=rwc";
    pub const DATABASE_BACKEND: &str = "sqlite";
    pub const JWT_SECRET: &str = "dev-secret-change-me";
    pub const JWT_EXPIRY_HOURS: i64 = 24;
    pub const SERVER_HOST: &str = "0.0.0.0";
    pub const SERVER_PORT: u16 = 8080;
    pub const DATA_DIR: &str = "./data";
}

/// Valid task statuses
pub const VALID_TASK_STATUSES: &[&str] = &["draft", "running", "paused", "completed", "failed"];
```

Update `from_env()` to use `defaults::*` constants instead of inline strings.

- [ ] **Step 2: Fix all clippy warnings**

Run: `cargo clippy --all-features -- -D warnings`
Fix:
- `redundant_clone` — pool clones needed for Arc creation: add `#[allow(clippy::redundant_clone)]` with comment
- `clone_on_copy` — remove `.clone()` on Copy types
- `too_many_arguments` — use builder pattern or param structs where appropriate

- [ ] **Step 3: Verify zero clippy warnings**

Run: `cargo clippy --all-features -- -D warnings`
Expected: Exit code 0, no output

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: extract constants, fix all clippy warnings"
```

---

### Task 11: Test coverage boost to 85%+

**Files:**
- Create: `crates/dts-server/tests/integration/` (API integration tests)
- Modify: Various `#[cfg(test)]` modules

- [ ] **Step 1: Add SQLite integration tests for all 8 repos** (if not already done in Tasks 3-4)
- [ ] **Step 2: Add service layer tests** — each service gets tests for success + error paths
- [ ] **Step 3: Add API integration tests** using `actix-web::test`

```rust
// crates/dts-server/tests/api/health_test.rs
use actix_web::{test, App};
use dts_server::app::health_check;

#[actix_web::test]
async fn health_endpoint_returns_ok() {
    let app = test::init_service(App::new().route("/health", web::get().to(health_check))).await;
    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}
```

- [ ] **Step 4: Run coverage check**

Run: `cargo llvm-cov --all-features --html`
Open the HTML report, identify gaps, add tests.

- [ ] **Step 5: Verify 85%+ coverage**

Run: `cargo llvm-cov --all-features --fail-under-lines 85`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "test: boost coverage to 85%+ with integration + API tests"
```

---

### Task 12: CI hardening

**Files:**
- Modify: `.github/workflows/` (CI configuration)

- [ ] **Step 1: Add coverage gate to CI**

```yaml
# In CI workflow, add:
- name: Run tests with coverage
  run: cargo llvm-cov --all-features --fail-under-lines 85 --lcov --output-path lcov.info
- name: Upload coverage
  uses: codecov/codecov-action@v4
  with:
    files: lcov.info
```

- [ ] **Step 2: Add clippy gate** (already partially there — ensure it includes all features)

```yaml
- name: Clippy check
  run: cargo clippy --all-features -- -D warnings
```

- [ ] **Step 3: Add file size check script**

```yaml
- name: Check file sizes
  run: |
    find crates -name '*.rs' -exec wc -l {} + | awk '$1 > 400 { print $2 ": " $1 " lines (max 400)"; exit 1 }'
```

- [ ] **Step 4: Push + verify CI passes**

- [ ] **Step 5: Commit**

---

## Phase 2 — Engine Integration

### Task 13: Create `dts-engine` crate — EngineService trait

**Files:**
- Create: `crates/dts-engine/Cargo.toml`
- Create: `crates/dts-engine/src/lib.rs`
- Create: `crates/dts-engine/src/error.rs`
- Create: `crates/dts-engine/src/model.rs`

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "dts-engine"
version = "0.1.0"
edition = "2021"
description = "DTS Engine — 数据同步引擎抽象层"

[dependencies]
dts-core = { path = "../dts-core" }
async-trait = { workspace = true }
tokio = { workspace = true, features = ["sync"] }
thiserror = { workspace = true }
serde = { workspace = true }
```

- [ ] **Step 2: Define EngineService trait + models**

```rust
// crates/dts-engine/src/lib.rs
pub mod error;
pub mod model;

pub use error::EngineError;
pub use model::*;

/// 引擎抽象 trait
#[async_trait::async_trait]
pub trait EngineService: Send + Sync {
    /// 提交同步任务，返回 run_id
    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError>;

    /// 取消运行中的任务
    async fn cancel(&self, run_id: &str) -> Result<(), EngineError>;
}
```

```rust
// crates/dts-engine/src/model.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TaskSubmission {
    pub task_id: String,
    pub source: ConnectionConfig,
    pub target: ConnectionConfig,
    pub config: SyncConfig,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub table_mappings: Vec<TableMapping>,
    pub mode: SyncMode,
    pub batch_size: usize,
}

#[derive(Debug, Clone)]
pub struct TableMapping {
    pub source_table: String,
    pub target_table: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    Full,
    Incremental,
    FullThenIncremental,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    pub run_id: String,
    pub task_id: String,
    pub status: RunStatus,
    pub rows_synced: u64,
    pub bytes_synced: u64,
    pub total_rows: Option<u64>,
    pub current_table: Option<String>,
    pub percent: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}
```

```rust
// crates/dts-engine/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("引擎内部错误: {0}")]
    Internal(String),

    #[error("任务不存在: {0}")]
    NotFound(String),

    #[error("任务已取消: {0}")]
    Cancelled(String),

    #[error("连接源数据库失败: {0}")]
    SourceConnection(String),

    #[error("连接目标数据库失败: {0}")]
    TargetConnection(String),
}
```

- [ ] **Step 3: Add to workspace, verify compilation**

Run: `cargo build -p dts-engine`
Expected: Compilation success

- [ ] **Step 4: Commit**

---

### Task 14: Create `dts-engine-ape` crate — ape-dts wrapper

**Files:**
- Create: `crates/dts-engine-ape/Cargo.toml`
- Create: `crates/dts-engine-ape/src/lib.rs`
- Create: `crates/dts-engine-ape/src/service.rs`
- Create: `crates/dts-engine-ape/src/adapter.rs`

- [ ] **Step 1: Write Cargo.toml** with ape-dts git dependency

```toml
[package]
name = "dts-engine-ape"
version = "0.1.0"
edition = "2021"
description = "ape-dts 引擎封装，实现 EngineService trait"

[dependencies]
dts-engine = { path = "../dts-engine" }
dts-core = { path = "../dts-core" }
async-trait = { workspace = true }
tokio = { workspace = true }
tracing = "0.1"
uuid = { workspace = true }
# ape-dts git dependency (exact crate names TBD after checking ape-dts repo)
# dt-task = { git = "https://github.com/apecloud/ape-dts", package = "dt-task" }
```

- [ ] **Step 2: Implement ApeEngineService** — skeleton with TODO for actual ape-dts call

```rust
// crates/dts-engine-ape/src/service.rs
use async_trait::async_trait;
use dts_engine::{EngineError, EngineService, SyncProgress, TaskSubmission};
use crate::adapter;

pub struct ApeEngineService;

#[async_trait]
impl EngineService for ApeEngineService {
    async fn submit(
        &self,
        task: TaskSubmission,
        progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
    ) -> Result<String, EngineError> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let task_clone = task.clone();
        let rid = run_id.clone();

        tokio::spawn(async move {
            if let Err(e) = adapter::run_sync(task_clone, &rid, progress_tx).await {
                tracing::error!(run_id = %rid, error = %e, "sync task failed");
            }
        });

        Ok(run_id)
    }

    async fn cancel(&self, run_id: &str) -> Result<(), EngineError> {
        tracing::warn!(run_id = %run_id, "cancel not yet implemented for ape-dts");
        Err(EngineError::Internal("cancel not implemented".into()))
    }
}
```

```rust
// crates/dts-engine-ape/src/adapter.rs
use dts_engine::{SyncProgress, RunStatus, TaskSubmission, EngineError};

/// Run sync via ape-dts library.
/// TODO: Replace this stub with actual dt-task::run() call
pub(crate) async fn run_sync(
    task: TaskSubmission,
    run_id: &str,
    progress_tx: tokio::sync::mpsc::UnboundedSender<SyncProgress>,
) -> Result<(), EngineError> {
    // Placeholder: send one progress update for now
    let _ = progress_tx.send(SyncProgress {
        run_id: run_id.into(),
        task_id: task.task_id.clone(),
        status: RunStatus::Running,
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: None,
        current_table: None,
        percent: 0.0,
        message: Some("Starting sync with ape-dts engine...".into()),
    });

    // TODO: Call actual ape-dts library here:
    // let config = build_ape_config(&task);
    // dt_task::run(config, progress_callback).await?;

    Ok(())
}
```

- [ ] **Step 3: Verify compilation** — `cargo build -p dts-engine-ape`

- [ ] **Step 4: Commit**

---

### Task 15: Add TaskRunRepo trait + implementations

**Files:**
- Modify: `crates/dts-backend/src/repo/mod.rs` — add `TaskRunRepo` trait
- Create: `crates/dts-backend/src/repo/task_run.rs`
- Create: `crates/dts-backend-postgres/src/repo/task_run.rs`
- Create: `crates/dts-backend-sqlite/src/repo/task_run.rs`

- [ ] **Step 1: Define TaskRunRepo trait**

```rust
// crates/dts-backend/src/repo/task_run.rs
use async_trait::async_trait;
use dts_core::types::TaskRunInfo;
use crate::BackendError;

#[derive(Debug, Clone)]
pub struct CreateTaskRunParams {
    pub id: String,
    pub task_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct UpdateTaskRunParams {
    pub run_id: String,
    pub status: String,
    pub rows_synced: i64,
    pub bytes_synced: i64,
    pub total_rows: Option<i64>,
    pub total_bytes: Option<i64>,
    pub end_time: Option<String>,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait TaskRunRepo: Send + Sync {
    async fn create(&self, params: &CreateTaskRunParams) -> Result<String, BackendError>;
    async fn update(&self, params: &UpdateTaskRunParams) -> Result<bool, BackendError>;
    async fn find_recent(&self, task_id: &str, limit: i64) -> Result<Vec<TaskRunInfo>, BackendError>;
}
```

- [ ] **Step 2: Implement for PostgreSQL + SQLite** — standard CRUD, following existing repo patterns

- [ ] **Step 3: Add to BackendContext** — `pub task_run_repo: Arc<dyn TaskRunRepo>`

- [ ] **Step 4: Run tests** — `cargo test --all`

- [ ] **Step 5: Commit**

---

### Task 16: Implement TaskScheduler + WebSocket progress

**Files:**
- Create: `crates/dts-server/src/services/scheduler.rs`
- Modify: `crates/dts-server/src/app.rs` — wire scheduler into app startup

- [ ] **Step 1: Implement TaskScheduler**

```rust
// crates/dts-server/src/services/scheduler.rs
use std::sync::Arc;
use dashmap::DashMap;
use dts_engine::{EngineService, SyncProgress, TaskSubmission};
use dts_backend::repo::{TaskRepo, TaskRunRepo};
use dts_backend::BackendError;
use crate::ws::task_log::WsHub;

pub struct TaskScheduler {
    engine: Arc<dyn EngineService>,
    task_repo: Arc<dyn TaskRepo>,
    task_run_repo: Arc<dyn TaskRunRepo>,
    ws_hub: Arc<WsHub>,
    active_runs: Arc<DashMap<String, ()>>,  // run_id → ()
}

impl TaskScheduler {
    pub fn new(
        engine: Arc<dyn EngineService>,
        task_repo: Arc<dyn TaskRepo>,
        task_run_repo: Arc<dyn TaskRunRepo>,
        ws_hub: Arc<WsHub>,
    ) -> Self {
        Self {
            engine,
            task_repo,
            task_run_repo,
            ws_hub,
            active_runs: Arc::new(DashMap::new()),
        }
    }

    /// Start a task synchronously
    pub async fn start_task(&self, task_id: &str, tenant_id: &str) -> Result<String, BackendError> {
        let task = self.task_repo
            .find_detail_by_id(task_id, tenant_id)
            .await?
            .ok_or_else(|| BackendError::NotFound("task not found".into()))?;

        let submission = self.build_submission(&task).await?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run_id = self.engine.submit(submission, tx).await
            .map_err(|e| BackendError::Internal(format!("engine error: {e}")))?;

        // Create run record
        self.task_run_repo.create(&dts_backend::repo::task_run::CreateTaskRunParams {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            status: "running".into(),
        }).await?;

        // Update task status
        self.task_repo.update_status(task_id, tenant_id, "running").await?;

        // Spawn progress listener
        let hub = self.ws_hub.clone();
        let run_repo = self.task_run_repo.clone();
        let task_repo = self.task_repo.clone();
        let tid = task_id.to_string();
        let tid2 = tenant_id.to_string();
        let rid = run_id.clone();

        tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                // Broadcast to WebSocket clients
                let msg = serde_json::to_string(&progress).unwrap_or_default();
                hub.broadcast(&tid, &msg);

                // Update run record
                let _ = run_repo.update(&dts_backend::repo::task_run::UpdateTaskRunParams {
                    run_id: rid.clone(),
                    status: format!("{:?}", progress.status).to_lowercase(),
                    rows_synced: progress.rows_synced as i64,
                    bytes_synced: progress.bytes_synced as i64,
                    total_rows: progress.total_rows.map(|v| v as i64),
                    total_bytes: None,
                    end_time: None,
                    error_message: progress.message.clone(),
                }).await;

                // Update task status on completion
                if matches!(progress.status, dts_engine::RunStatus::Completed) {
                    let _ = task_repo.update_status(&tid, &tid2, "completed").await;
                } else if matches!(progress.status, dts_engine::RunStatus::Failed) {
                    let _ = task_repo.update_status(&tid, &tid2, "failed").await;
                }
            }
        });

        Ok(run_id)
    }

    async fn build_submission(&self, task: &dts_core::types::TaskDetail) -> Result<TaskSubmission, BackendError> {
        // Parse task config and build submission from datasource configs
        todo!("Build TaskSubmission from task detail")
    }
}
```

- [ ] **Step 2: Wire scheduler in `app.rs`** — create `ApeEngineService` (or mock for now), wrap in `TaskScheduler`, register as app data

- [ ] **Step 3: Add `POST /api/v1/tasks/{id}/start` endpoint** — calls `scheduler.start_task()`

- [ ] **Step 4: Ensure WebSocket hub has `broadcast` method**

Check `crates/dts-server/src/ws/task_log.rs` — if `broadcast` doesn't exist, add:

```rust
impl WsHub {
    pub fn broadcast(&self, task_id: &str, message: &str) {
        // Send to all connected clients for this task_id
    }
}
```

- [ ] **Step 5: Integration test** — mock engine, create task, start, verify run_record written + WS message sent

- [ ] **Step 6: Commit**

---

### Task 17: Full integration test + end-to-end verification

- [ ] **Step 1: Start server with SQLite backend**

Run: `cargo run -p dts-server`
Expected: Server starts, migrations run, seed data inserted

- [ ] **Step 2: Test API endpoints with curl**

```bash
# Login
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"tenant_slug":"default","username":"admin","password":"admin123"}'

# List tasks (with JWT from login)
curl http://localhost:8080/api/v1/tasks \
  -H "Authorization: Bearer <token>"

# Health check
curl http://localhost:8080/health
```

Expected: All endpoints return 200 OK with valid JSON

- [ ] **Step 3: Run full test suite with coverage**

Run: `cargo llvm-cov --all-features --fail-under-lines 85`
Expected: PASS

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --all-features -- -D warnings`
Expected: Zero warnings

- [ ] **Step 5: Final commit + push**

```bash
git add -A
git commit -m "feat: complete engine integration with scheduler + WS progress + integration tests"
git push -u origin feat/supabase-postgresql
```

---

## Summary

| Phase | Tasks | Key Deliverables |
|-------|-------|-----------------|
| Phase 1 | Tasks 1–12 | dts-core, dts-backend-sqlite, 6 services, tracing, 85% coverage, CI hardened |
| Phase 2 | Tasks 13–17 | dts-engine, dts-engine-ape, TaskScheduler, TaskRunRepo, WS progress, E2E verified |
