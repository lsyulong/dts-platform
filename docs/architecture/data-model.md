# DTS Platform — 数据模型设计

## ER 图

```
tenant (租户) 1 ──── N user (用户)
tenant 1 ──── N role (角色)
tenant 1 ──── N datasource (数据源)
tenant 1 ──── N sync_task (同步任务)
tenant 1 ──── N audit_log (审计日志)

user N ──── M role (通过 user_role)
role N ──── M permission (通过 role_permission)

sync_task 1 ──── N task_run (执行历史)
sync_task 1 ──── N alert_rule (告警规则)
sync_task N ──── 2 datasource (source + target)
```

## 表定义

### tenant — 租户表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| name | TEXT NOT NULL | 租户名称 |
| slug | TEXT NOT NULL UNIQUE | URL 友好标识 |
| status | TEXT NOT NULL DEFAULT 'active' | active / suspended / deleted |
| created_at | TEXT NOT NULL | ISO 8601 时间戳 |
| updated_at | TEXT NOT NULL | ISO 8601 时间戳 |

### user — 用户表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| username | TEXT NOT NULL | 用户名 |
| email | TEXT NOT NULL | 邮箱 |
| password_hash | TEXT NOT NULL | argon2 哈希 |
| display_name | TEXT | 显示名称 |
| status | TEXT NOT NULL DEFAULT 'active' | active / disabled |
| created_at | TEXT NOT NULL | ISO 8601 时间戳 |
| updated_at | TEXT NOT NULL | ISO 8601 时间戳 |

UNIQUE: (tenant_id, username), (tenant_id, email)

### role — 角色表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| name | TEXT NOT NULL | 角色名称 |
| description | TEXT | 角色描述 |
| is_system | INTEGER NOT NULL DEFAULT 0 | 是否系统内置角色 |
| created_at | TEXT NOT NULL | ISO 8601 时间戳 |

UNIQUE: (tenant_id, name)

### user_role — 用户角色关联表

| 列名 | 类型 | 说明 |
|------|------|------|
| user_id | TEXT NOT NULL | 外键 → user.id |
| role_id | TEXT NOT NULL | 外键 → role.id |

PRIMARY KEY: (user_id, role_id)

### role_permission — 角色权限表

| 列名 | 类型 | 说明 |
|------|------|------|
| role_id | TEXT NOT NULL | 外键 → role.id |
| permission | TEXT NOT NULL | 权限标识符 |

PRIMARY KEY: (role_id, permission)

### datasource — 数据源表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| name | TEXT NOT NULL | 连接名称 |
| db_type | TEXT NOT NULL | MySQL/PostgreSQL/MongoDB/Redis/Kafka... |
| host | TEXT NOT NULL | 主机地址 |
| port | INTEGER NOT NULL | 端口 |
| username | TEXT | 用户名 |
| encrypted_password | TEXT | 加密后的密码 |
| database_name | TEXT | 数据库名 |
| extra_params | TEXT | JSON 格式额外参数 |
| health_status | TEXT DEFAULT 'unknown' | healthy / unhealthy / unknown |
| last_test_at | TEXT | 上次测试时间 |
| created_by | TEXT NOT NULL | 创建者 user.id |
| created_at | TEXT NOT NULL | ISO 8601 |
| updated_at | TEXT NOT NULL | ISO 8601 |

### sync_task — 同步任务表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| name | TEXT NOT NULL | 任务名称 |
| description | TEXT | 任务描述 |
| source_id | TEXT NOT NULL | 外键 → datasource.id |
| target_id | TEXT NOT NULL | 外键 → datasource.id |
| cron_expr | TEXT | Cron 表达式，为空则手动触发 |
| status | TEXT NOT NULL DEFAULT 'draft' | draft/running/paused/completed/failed |
| config_json | TEXT NOT NULL | JSON 格式任务配置(表映射/字段映射/过滤规则等) |
| created_by | TEXT NOT NULL | 创建者 user.id |
| created_at | TEXT NOT NULL | ISO 8601 |
| updated_at | TEXT NOT NULL | ISO 8601 |

### task_run — 任务执行历史表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| task_id | TEXT NOT NULL | 外键 → sync_task.id |
| status | TEXT NOT NULL | running/completed/failed/cancelled |
| start_time | TEXT NOT NULL | ISO 8601 |
| end_time | TEXT | ISO 8601 |
| rows_synced | INTEGER DEFAULT 0 | 已同步行数 |
| bytes_synced | INTEGER DEFAULT 0 | 已同步字节数 |
| total_rows | INTEGER | 总行数 |
| total_bytes | INTEGER | 总字节数 |
| error_message | TEXT | 错误信息 |
| created_at | TEXT NOT NULL | ISO 8601 |

CREATE INDEX: (task_id, start_time DESC)

### alert_rule — 告警规则表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| task_id | TEXT | 外键 → sync_task.id (NULL=全局规则) |
| name | TEXT NOT NULL | 规则名称 |
| metric_type | TEXT NOT NULL | latency/error_rate/throughput/row_count |
| operator | TEXT NOT NULL | gt/lt/gte/lte/eq |
| threshold | REAL NOT NULL | 阈值 |
| enabled | INTEGER NOT NULL DEFAULT 1 | 是否启用 |
| notify_channels | TEXT | JSON: ["slack","email","webhook"] |
| created_at | TEXT NOT NULL | ISO 8601 |
| updated_at | TEXT NOT NULL | ISO 8601 |

### audit_log — 审计日志表

| 列名 | 类型 | 说明 |
|------|------|------|
| id | TEXT PRIMARY KEY | UUID v4 |
| tenant_id | TEXT NOT NULL | 外键 → tenant.id |
| user_id | TEXT NOT NULL | 外键 → user.id |
| action | TEXT NOT NULL | 操作类型 |
| resource | TEXT NOT NULL | 资源标识 |
| detail | TEXT | JSON 格式详情 |
| ip_address | TEXT | 客户端 IP |
| created_at | TEXT NOT NULL | ISO 8601 |

CREATE INDEX: (tenant_id, created_at DESC)

## RBAC 权限矩阵

### 预定义角色

| 角色 | 说明 | 是否系统内置 |
|------|------|------------|
| super_admin | 平台管理员，跨租户 | 是 |
| tenant_admin | 租户管理员 | 是 |
| developer | 开发运维，创建/编辑任务 | 是 |
| viewer | 只读用户 | 是 |

### 权限定义

| 权限标识 | 说明 |
|---------|------|
| `task:read` | 查看任务列表和详情 |
| `task:write` | 创建/编辑/删除任务 |
| `task:execute` | 启动/停止/暂停任务 |
| `datasource:read` | 查看数据源 |
| `datasource:write` | 创建/编辑/删除数据源 |
| `monitor:read` | 查看监控指标和日志 |
| `alert:read` | 查看告警规则 |
| `alert:write` | 创建/编辑告警规则 |
| `user:manage` | 管理租户内用户和角色 |
| `tenant:manage` | 管理租户配置 |
| `audit:read` | 查看审计日志 |

### 角色-权限映射

| 权限 | super_admin | tenant_admin | developer | viewer |
|------|:--:|:--:|:--:|:--:|
| `task:read` | ✅ | ✅ | ✅ | ✅ |
| `task:write` | ✅ | ✅ | ✅ | ❌ |
| `task:execute` | ✅ | ✅ | ✅ | ❌ |
| `datasource:read` | ✅ | ✅ | ✅ | ✅ |
| `datasource:write` | ✅ | ✅ | ✅ | ❌ |
| `monitor:read` | ✅ | ✅ | ✅ | ✅ |
| `alert:read` | ✅ | ✅ | ✅ | ✅ |
| `alert:write` | ✅ | ✅ | ❌ | ❌ |
| `user:manage` | ✅ | ✅ | ❌ | ❌ |
| `tenant:manage` | ✅ | ❌ | ❌ | ❌ |
| `audit:read` | ✅ | ✅ | ❌ | ❌ |
