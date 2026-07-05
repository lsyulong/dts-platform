-- DTS Platform - Initial Database Schema (PostgreSQL / Supabase)

-- Table: tenant (租户)
CREATE TABLE IF NOT EXISTS tenant (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table: "user" (用户) — quoted because user is a reserved word in PostgreSQL
CREATE TABLE IF NOT EXISTS "user" (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, username),
    UNIQUE(tenant_id, email)
);

-- Table: role (角色)
CREATE TABLE IF NOT EXISTS role (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

-- Table: user_role (用户-角色关联)
CREATE TABLE IF NOT EXISTS user_role (
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

-- Table: role_permission (角色-权限关联)
CREATE TABLE IF NOT EXISTS role_permission (
    role_id TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    PRIMARY KEY (role_id, permission)
);

-- Table: datasource (数据源连接)
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
    last_test_at TIMESTAMPTZ,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table: sync_task (同步任务)
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table: task_run (任务执行历史)
CREATE TABLE IF NOT EXISTS task_run (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES sync_task(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'running',
    start_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    end_time TIMESTAMPTZ,
    rows_synced BIGINT NOT NULL DEFAULT 0,
    bytes_synced BIGINT NOT NULL DEFAULT 0,
    total_rows BIGINT DEFAULT 0,
    total_bytes BIGINT DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_task_run_task_id ON task_run(task_id, start_time DESC);

-- Table: alert_rule (告警规则)
CREATE TABLE IF NOT EXISTS alert_rule (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES sync_task(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    metric_type TEXT NOT NULL,
    operator TEXT NOT NULL,
    threshold DOUBLE PRECISION NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    notify_channels TEXT DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table: audit_log (审计日志)
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    detail TEXT DEFAULT '{}',
    ip_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_log_tenant ON audit_log(tenant_id, created_at DESC);