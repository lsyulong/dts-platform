# DTS Platform — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从零构建 dts-platform 企业级数据同步管理平台，包含 Vite+React 前端、actix-web 后端、RBAC 多租户、SQLite 持久化、ape-dts 引擎集成。

**Architecture:** 独立仓库 dts-platform，通过 Git 依赖引用 ape-dts 引擎。分层架构：API Layer → Service Layer → Repository Layer → Infrastructure Layer。EngineService trait 抽象引擎调用。

**Tech Stack:** Rust (actix-web, sqlx, sqlite), TypeScript (Vite, React, shadcn/ui, Zustand, Recharts)

---

## Phase 1: 项目骨架搭建

### Task 1.1: 初始化 Rust 项目结构

**Files:**
- Create: `Cargo.toml`
- Create: `crates/dts-server/Cargo.toml`
- Create: `.env.example`
- Create: `.gitignore`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: 创建 workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/dts-server"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
anyhow = "1"
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 2: 创建 dts-server Cargo.toml**

```toml
[package]
name = "dts-server"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
actix-cors = "0.7"
actix-ws = "0.3"
actix-rt = "2"
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true }
uuid = { workspace = true, features = ["v4"] }
chrono = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
log = { workspace = true }
env_logger = { workspace = true }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite", "chrono", "uuid"] }
jsonwebtoken = "9"
argon2 = "0.5"
dotenv = "0.15"
dashmap = "6"
async-trait = "0.1"
argon2 = "0.5"
rand = "0.8"
```

- [ ] **Step 3: 创建 .env.example**

```bash
DATABASE_URL=sqlite:./data/dts.db?mode=rwc
JWT_SECRET=change-me-to-a-random-secret
JWT_EXPIRY_HOURS=24
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
RUST_LOG=info,dts_server=debug
```

- [ ] **Step 4: 创建 .gitignore**

```
/target/
/data/
.env
node_modules/
dist/
.vite/
*.db
*.db-journal
*.db-wal
```

- [ ] **Step 5: 创建 rust-toolchain.toml**

```toml
[toolchain]
channel = "1.85.0"
```

- [ ] **Step 6: 验证编译**

```bash
cd D:/rustpro/dts-platform
cargo check
```

Expected: 编译成功 (无源文件，仅依赖下载)

---

### Task 1.2: 初始化前端项目

**Files:**
- Create: `web/package.json`
- Create: `web/vite.config.ts`
- Create: `web/tsconfig.json`
- Create: `web/tsconfig.node.json`
- Create: `web/tailwind.config.ts`
- Create: `web/postcss.config.js`
- Create: `web/index.html`
- Create: `web/src/main.tsx`
- Create: `web/src/App.tsx`
- Create: `web/src/index.css`

- [ ] **Step 1: 初始化 Vite + React + TypeScript 项目**

```bash
cd D:/rustpro/dts-platform/web
pnpm create vite . --template react-ts
pnpm install
```

- [ ] **Step 2: 安装前端依赖**

```bash
cd D:/rustpro/dts-platform/web
pnpm add react-router-dom zustand axios recharts lucide-react
pnpm add -D tailwindcss @tailwindcss/vite postcss autoprefixer
pnpm add -D @types/node
```

- [ ] **Step 3: 配置 Tailwind CSS 和 shadcn/ui**

按照 shadcn/ui 官方文档初始化:
```bash
npx shadcn@latest init
```
选择: TypeScript, Neutral 配色, CSS variables: yes

- [ ] **Step 4: 安装 shadcn/ui 基础组件**

```bash
npx shadcn@latest add button card input label table dialog tabs select dropdown-menu badge separator scroll-area tooltip avatar toast sheet
```

- [ ] **Step 5: 创建 src/index.css 全局样式**

```css
@import "tailwindcss";

:root {
  --background: 0 0% 100%;
  --foreground: 240 10% 3.9%;
  /* ... CSS variables for light/dark theme */
}

.dark {
  --background: 240 10% 3.9%;
  --foreground: 0 0% 98%;
  /* ... dark theme variables */
}
```

- [ ] **Step 6: 验证启动**

```bash
cd D:/rustpro/dts-platform/web
pnpm dev
```

Expected: 浏览器打开 http://localhost:5173 看到 React 默认页面

---

## Phase 2: 后端核心基础设施

### Task 2.1: 配置管理 + 数据库连接池

**Files:**
- Create: `crates/dts-server/src/config.rs`
- Create: `crates/dts-server/src/db/mod.rs`
- Create: `crates/dts-server/src/db/pool.rs`

- [ ] **Step 1: 实现 config.rs — 应用配置结构体，从环境变量加载**

```rust
// crates/dts-server/src/config.rs
use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub server_host: String,
    pub server_port: u16,
    pub data_dir: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./data/dts.db?mode=rwc".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()?,
            server_host: env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()?,
            data_dir: "./data".to_string(),
        })
    }
}
```

- [ ] **Step 2: 实现 db/pool.rs — SQLite 连接池初始化**

```rust
// crates/dts-server/src/db/pool.rs
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

pub async fn create_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    // 确保 data 目录存在
    if database_url.starts_with("sqlite:./data/") {
        tokio::fs::create_dir_all("./data").await?;
    }

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;

    Ok(pool)
}
```

- [ ] **Step 3: 实现 db/mod.rs — 数据库迁移**

```rust
// crates/dts-server/src/db/mod.rs
pub mod pool;
use sqlx::SqlitePool;

pub use pool::create_pool;

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
```

- [ ] **Step 4: 验证编译通过**

```bash
cd D:/rustpro/dts-platform
cargo check -p dts-server
```

---

### Task 2.2: 数据库迁移 SQL

**Files:**
- Create: `crates/dts-server/migrations/001_initial_schema.sql`

- [ ] **Step 1: 创建完整的初始迁移 SQL**

```sql
-- crates/dts-server/migrations/001_initial_schema.sql

-- 租户表
CREATE TABLE IF NOT EXISTS tenant (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 用户表
CREATE TABLE IF NOT EXISTS user (
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

-- 角色表
CREATE TABLE IF NOT EXISTS role (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

-- 用户角色关联表
CREATE TABLE IF NOT EXISTS user_role (
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

-- 角色权限表
CREATE TABLE IF NOT EXISTS role_permission (
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    PRIMARY KEY (role_id, permission)
);

-- 数据源表
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

-- 同步任务表
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

-- 任务执行历史表
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

-- 告警规则表
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

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    detail TEXT DEFAULT '{}',
    ip_address TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_log_tenant ON audit_log(tenant_id, created_at DESC);
```

---

## Phase 3: 后端认证与 RBAC

### Task 3.1: JWT 认证 + 密码哈希

**Files:**
- Create: `crates/dts-server/src/services/auth_svc.rs`
- Create: `crates/dts-server/src/models/auth.rs`

- [ ] **Step 1: 创建 auth models**

```rust
// crates/dts-server/src/models/auth.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub tenant_slug: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub tenant_id: String,
    pub tenant_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // user_id
    pub tenant_id: String,
    pub username: String,
    pub exp: usize,
    pub iat: usize,
}
```

- [ ] **Step 2: 实现 auth_svc.rs — 核心认证逻辑**

```rust
// crates/dts-server/src/services/auth_svc.rs
use crate::config::AppConfig;
use crate::models::auth::*;
use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use sqlx::SqlitePool;
use std::sync::Arc;

pub struct AuthService {
    pool: SqlitePool,
    config: AppConfig,
}

impl AuthService {
    pub fn new(pool: SqlitePool, config: AppConfig) -> Self {
        Self { pool, config }
    }

    pub async fn login(&self, req: &LoginRequest) -> Result<LoginResponse> {
        // 1. 查找租户
        let tenant = sqlx::query!(
            "SELECT id, name, status FROM tenant WHERE slug = ? AND status = 'active'",
            req.tenant_slug
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("租户不存在或已停用"))?;

        // 2. 查找用户
        let user = sqlx::query!(
            "SELECT id, username, email, password_hash, display_name, status
             FROM user WHERE tenant_id = ? AND username = ?",
            tenant.id, req.username
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow!("用户名或密码错误"))?;

        if user.status != "active" {
            return Err(anyhow!("账户已被禁用"));
        }

        // 3. 验证密码
        let parsed_hash = PasswordHash::new(&user.password_hash)
            .map_err(|_| anyhow!("密码哈希无效"))?;
        Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .map_err(|_| anyhow!("用户名或密码错误"))?;

        // 4. 获取角色和权限
        let roles: Vec<String> = sqlx::query_scalar!(
            "SELECT r.name FROM role r
             INNER JOIN user_role ur ON r.id = ur.role_id
             WHERE ur.user_id = ?",
            user.id
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|r| r)
        .collect();

        let permissions: Vec<String> = sqlx::query_scalar!(
            "SELECT DISTINCT rp.permission FROM role_permission rp
             INNER JOIN user_role ur ON rp.role_id = ur.role_id
             WHERE ur.user_id = ?",
            user.id
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|p| p)
        .collect();

        // 5. 生成 JWT
        let claims = Claims {
            sub: user.id.clone(),
            tenant_id: tenant.id.clone(),
            username: user.username.clone(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(self.config.jwt_expiry_hours))
                .timestamp() as usize,
            iat: chrono::Utc::now().timestamp() as usize,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )?;

        Ok(LoginResponse {
            token,
            user: UserInfo {
                id: user.id,
                username: user.username,
                email: user.email,
                display_name: user.display_name,
                tenant_id: tenant.id,
                tenant_name: tenant.name,
                roles,
                permissions,
            },
        })
    }

    pub async fn hash_password(password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {}", e))?;
        Ok(hash.to_string())
    }

    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )?;
        Ok(token_data.claims)
    }

    pub fn hash_password_sync(password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow!("密码哈希失败: {}", e))?;
        Ok(hash.to_string())
    }
}
```

- [ ] **Step 3: 验证编译**

```bash
cd D:/rustpro/dts-platform
cargo check -p dts-server
```

---

### Task 3.2: JWT 认证中间件 + 租户隔离中间件

**Files:**
- Create: `crates/dts-server/src/middleware/mod.rs`
- Create: `crates/dts-server/src/middleware/auth.rs`
- Create: `crates/dts-server/src/middleware/tenant.rs`
- Create: `crates/dts-server/src/models/error.rs`

- [ ] **Step 1: 创建错误模型**

```rust
// crates/dts-server/src/models/error.rs
use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            Self::NotFound(msg) => write!(f, "Not Found: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            Self::Internal(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status, code) = match self {
            Self::Unauthorized(_) => (actix_web::http::StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Forbidden(_) => (actix_web::http::StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::NotFound(_) => (actix_web::http::StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::BadRequest(_) => (actix_web::http::StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            Self::Internal(_) => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
            ),
        };

        HttpResponse::build(status).json(ErrorResponse {
            code: code.to_string(),
            message: self.to_string(),
        })
    }
}
```

- [ ] **Step 2: 创建 JWT 认证中间件**

```rust
// crates/dts-server/src/middleware/auth.rs
use crate::models::auth::Claims;
use crate::models::error::AppError;
use actix_web::{dev::ServiceRequest, Error};
use jsonwebtoken::{decode, DecodingKey, Validation};

pub fn extract_claims(req: &ServiceRequest, jwt_secret: &str) -> Result<Claims, AppError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("缺少 Authorization 头".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("无效的 Authorization 格式".into()))?;

    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| AppError::Unauthorized(format!("Token 验证失败: {}", e)))?;

    Ok(claims.claims)
}
```

- [ ] **Step 3: 创建租户隔离中间件**

```rust
// crates/dts-server/src/middleware/tenant.rs
use crate::models::auth::Claims;
use crate::models::error::AppError;

/// 验证请求者是否属于指定租户
pub fn verify_tenant_access(claims: &Claims, target_tenant_id: &str) -> Result<(), AppError> {
    // super_admin 可以跨租户访问
    if claims.tenant_id == target_tenant_id {
        return Ok(());
    }
    Err(AppError::Forbidden("跨租户访问被拒绝".into()))
}

/// 从 Claims 中提取租户 ID (用于自动设置 tenant 过滤)
pub fn tenant_id_from_claims(claims: &Claims) -> String {
    claims.tenant_id.clone()
}
```

- [ ] **Step 4: 创建中间件模块导出**

```rust
// crates/dts-server/src/middleware/mod.rs
pub mod auth;
pub mod tenant;
```

---

### Task 3.3: 认证 API 路由

**Files:**
- Create: `crates/dts-server/src/api/mod.rs`
- Create: `crates/dts-server/src/api/auth.rs`
- Create: `crates/dts-server/src/models/mod.rs`
- Create: `crates/dts-server/src/models/auth.rs`

- [ ] **Step 1: 创建 models 模块**

```rust
// crates/dts-server/src/models/mod.rs
pub mod auth;
pub mod error;
```

- [ ] **Step 2: 实现 auth API handlers**

```rust
// crates/dts-server/src/api/auth.rs
use actix_web::{web, HttpResponse, Responder};
use crate::models::auth::*;
use crate::models::error::AppError;
use crate::services::auth_svc::AuthService;

pub async fn login(
    auth_svc: web::Data<AuthService>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let response = auth_svc
        .login(&body)
        .await
        .map_err(|e| AppError::Unauthorized(e.to_string()))?;
    Ok(HttpResponse::Ok().json(response))
}

pub async fn me(
    auth_svc: web::Data<AuthService>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse, AppError> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("缺少 Token".into()))?;

    let claims = auth_svc
        .verify_token(token)
        .map_err(|e| AppError::Unauthorized(e.to_string()))?;

    // 查询用户信息
    use sqlx::SqlitePool;
    let pool = web::Data::<SqlitePool>::from(req.app_data().unwrap());
    let user = sqlx::query!(
        "SELECT u.id, u.username, u.email, u.display_name, t.name as tenant_name
         FROM user u JOIN tenant t ON u.tenant_id = t.id
         WHERE u.id = ?",
        claims.sub
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(|_| AppError::NotFound("用户不存在".into()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": user.id,
        "username": user.username,
        "email": user.email,
        "display_name": user.display_name,
        "tenant_name": user.tenant_name,
    })))
}
```

- [ ] **Step 3: 创建 api 模块导出**

```rust
// crates/dts-server/src/api/mod.rs
pub mod auth;
```

---

## Phase 4: 后端业务 API

### Task 4.1: 租户管理 API

**Files:**
- Create: `crates/dts-server/src/api/tenants.rs`
- Create: `crates/dts-server/src/services/tenant_svc.rs`

- [ ] **Step 1: 实现 TenantService**

```rust
// crates/dts-server/src/services/tenant_svc.rs
use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TenantService {
    pool: SqlitePool,
}

impl TenantService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, name: &str, slug: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query!(
            "INSERT INTO tenant (id, name, slug) VALUES (?, ?, ?)",
            id, name, slug
        )
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list(&self) -> Result<Vec<TenantInfo>> {
        // 需要重导出类型
        todo!()
    }
}
```

- [ ] **Step 2: 实现租户 CRUD handlers**

```rust
// crates/dts-server/src/api/tenants.rs
use actix_web::{web, HttpResponse, Responder};
use crate::models::error::AppError;
use crate::services::tenant_svc::TenantService;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
}

pub async fn create(
    svc: web::Data<TenantService>,
    body: web::Json<CreateTenantRequest>,
) -> Result<HttpResponse, AppError> {
    // 验证 slug 是否已存在
    let id = svc
        .create(&body.name, &body.slug)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": body.name,
        "slug": body.slug,
    })))
}
```

---

### Task 4.2: 用户管理 + 角色管理 API

**Files:**
- Create: `crates/dts-server/src/api/users.rs`
- Create: `crates/dts-server/src/api/roles.rs`
- Create: `crates/dts-server/src/services/user_svc.rs`

---

### Task 4.3: 数据源管理 API

**Files:**
- Create: `crates/dts-server/src/api/datasources.rs`
- Create: `crates/dts-server/src/services/datasource_svc.rs`

---

### Task 4.4: 同步任务管理 API

**Files:**
- Create: `crates/dts-server/src/api/tasks.rs`
- Create: `crates/dts-server/src/services/task_svc.rs`

---

### Task 4.5: 监控 + 告警 API

**Files:**
- Create: `crates/dts-server/src/api/monitoring.rs`
- Create: `crates/dts-server/src/api/alerts.rs`

---

### Task 4.6: WebSocket 实时日志推送

**Files:**
- Create: `crates/dts-server/src/ws/mod.rs`
- Create: `crates/dts-server/src/ws/task_log.rs`

---

### Task 4.7: 组装 app.rs 注册所有路由和中间件

**Files:**
- Create: `crates/dts-server/src/app.rs`
- Modify: `crates/dts-server/src/main.rs`

- [ ] **Step 1: 实现 main.rs 启动入口**

```rust
// crates/dts-server/src/main.rs
mod api;
mod app;
mod config;
mod db;
mod middleware;
mod models;
mod services;
mod ws;

use app::create_app;
use config::AppConfig;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let config = AppConfig::from_env()?;
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;

    log::info!("数据库迁移完成");

    let app = create_app(pool, config).await?;
    log::info!("服务启动中...");
    app.await?;

    Ok(())
}
```

- [ ] **Step 2: 实现 app.rs — 路由配置**

```rust
// crates/dts-server/src/app.rs
use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use sqlx::SqlitePool;
use crate::config::AppConfig;
use crate::services::auth_svc::AuthService;
use crate::services::tenant_svc::TenantService;

pub async fn create_app(
    pool: SqlitePool,
    config: AppConfig,
) -> anyhow::Result<actix_web::dev::Server> {
    let auth_svc = AuthService::new(pool.clone(), config.clone());
    let tenant_svc = TenantService::new(pool.clone());

    let bind_addr = format!("{}:{}", config.server_host, config.server_port);
    let server = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_middleware::Logger::default())
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(auth_svc.clone()))
            .app_data(web::Data::new(tenant_svc.clone()))
            .service(
                web::scope("/api/v1")
                    .route("/auth/login", web::post().to(crate::api::auth::login))
                    .route("/auth/me", web::get().to(crate::api::auth::me))
                    .route("/tenants", web::post().to(crate::api::tenants::create))
                    // 后续继续添加路由...
            )
            .route("/health", web::get().to(health_check))
    })
    .bind(&bind_addr)?
    .run();

    Ok(server)
}

async fn health_check() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "dts-platform"
    }))
}
```

---

## Phase 5: 前端实现

### Task 5.1: 布局组件 + 路由 + 状态管理

**Files:**
- Create: `web/src/router.tsx`
- Create: `web/src/layouts/DashboardLayout.tsx`
- Create: `web/src/components/Sidebar.tsx`
- Create: `web/src/components/Header.tsx`
- Create: `web/src/stores/auth.ts`
- Create: `web/src/stores/app.ts`
- Create: `web/src/api/client.ts`

### Task 5.2: 登录页面

**Files:**
- Create: `web/src/pages/login.tsx`

### Task 5.3: 仪表盘页面

**Files:**
- Create: `web/src/pages/dashboard.tsx`
- Create: `web/src/components/MetricCard.tsx`
- Create: `web/src/components/DataFlowTopology.tsx`
- Create: `web/src/components/LogConsole.tsx`

### Task 5.4: 任务管理页面

**Files:**
- Create: `web/src/pages/tasks/index.tsx`
- Create: `web/src/components/TaskTable.tsx`

### Task 5.5: 任务创建向导页面

**Files:**
- Create: `web/src/pages/tasks/create.tsx`

### Task 5.6: 任务详情页面

**Files:**
- Create: `web/src/pages/tasks/detail.tsx`
- Create: `web/src/components/TaskProgressBar.tsx`

### Task 5.7: 数据源管理页面

**Files:**
- Create: `web/src/pages/datasources.tsx`
- Create: `web/src/components/DataSourceForm.tsx`

### Task 5.8: 监控告警页面

**Files:**
- Create: `web/src/pages/monitoring.tsx`
- Create: `web/src/components/HealthPanel.tsx`
- Create: `web/src/components/AlertRuleConfig.tsx`

---

## Phase 6: 集成测试 + 本地验证

### Task 6.1: 后端集成测试

### Task 6.2: 端到端验证 + 种子数据

### Task 6.3: 生产构建验证

---

## 实现说明

由于本计划非常宏大（涉及 30+ 文件），建议采用以下策略：

1. **优先实现核心路径**：登录认证 → 仪表盘 → 任务 CRUD → 数据源管理
2. **前端页面逐页实现**：每完成一个后端 API 模块，立即实现对应的前端页面
3. **复用参考 HTML 的设计**：现有 `数据同步管理平台` 的 6 个 HTML 文件作为 UI 参考，直接迁移组件结构和样式
