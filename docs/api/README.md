# DTS Platform API Reference

Base URL: `/api/v1`

## Authentication

User-facing business endpoints require an opaque Sa-Token session token. `/auth/login`
and the root `/health` endpoint are public. Internal scheduler callbacks use their
separate task-scoped machine authorization.

```
Authorization: Bearer <token>
```

The token is obtained via `POST /api/v1/auth/login`. Its lifetime is controlled by
`AUTH_TOKEN_TIMEOUT_SECS` (default: 86400 seconds). Old JWTs are rejected after this
upgrade. In-memory sessions expire on server restart; shared Redis sessions require
the `auth-redis` build feature and `AUTH_STORAGE=redis`.

---

## Auth

### `POST /auth/login`

Login with tenant slug, username, and password.

**Request:**
```json
{
  "tenant_slug": "default",
  "username": "admin",
  "password": "<administrator-provided-password>"
}
```

**Response (200):**
```json
{
  "token": "<64-character opaque token>",
  "user": {
    "id": "...",
    "username": "admin",
    "display_name": "Admin",
    "tenant_id": "...",
    "tenant_name": "Default",
    "roles": ["tenant_admin"],
    "permissions": ["datasource:read", "task:read"]
  }
}
```

### `GET /auth/me`

Get current user info from a valid session. Requires the `Authorization` header.
Every request checks the current account and tenant status; role permissions are
resolved from the control-plane database.

### `POST /auth/logout`

Revoke the current session. Requires the `Authorization` header and returns `204`
without a response body. Reusing that token returns `401`. Password resets revoke
the user's previous sessions; disabled accounts cannot authenticate. Existing task
log WebSockets recheck authentication and authorization on their 15-second heartbeat.

### `GET /auth/permissions`

Get current user's permission list.

---

## Tenants

### `POST /tenants`

Create a tenant. Requires `tenant:create`. Supply an initial administrator password
of at least 12 bytes; it is never returned in the response.

**Request:**
```json
{
  "name": "My Tenant",
  "slug": "my-tenant",
  "admin_password": "<new-tenant-administrator-password>"
}
```

### `GET /tenants`

Get the current tenant as a list. Requires `user:manage`.

---

## Data Sources

### `GET /datasources`

List data sources for the current tenant.

### `POST /datasources`

Create a new data source connection.

**Request:**
```json
{
  "name": "Production DB",
  "db_type": "postgresql",
  "host": "db.example.com",
  "port": 5432,
  "username": "reader",
  "password": "secret",
  "database_name": "production"
}
```

### `GET /datasources/{id}`

Get data source details.

### `POST /datasources/{id}/test`

Run a live `SELECT 1` connection probe and persist its result. Requires
`datasource:write`. MySQL, PostgreSQL, SQLite, and DuckDB through SwanLake are supported; other types
return an explicit `unknown` result instead of reporting a false success.

### `GET /datasources/{id}/probes?limit=20`

List the newest persisted connection probes. Requires `datasource:read`; the
limit must be between 1 and 100.

### `GET /datasources/{id}/tables`

Read at most 500 base tables from the live MySQL, PostgreSQL, or DuckDB/SwanLake
catalog. Requires `datasource:read`. Credentials stay in the protected backend
repository and are never returned to the caller.

### `DELETE /datasources/{id}`

Delete a data source. Fails if referenced by active tasks.

---

## Tasks

### `GET /tasks`

List all sync tasks for the current tenant.

### `POST /tasks`

Create a sync task.

**Request:**
```json
{
  "name": "MySQL to DuckDB Sync",
  "description": "Hourly sync of user data",
  "source_id": "<datasource-id>",
  "target_id": "<datasource-id>",
  "cron_expr": null,
  "sync_mode": "full",
  "batch_size": 1000,
  "table_mappings": [
    {
      "source_schema": "sales",
      "source_table": "customers",
      "target_schema": "main",
      "target_table": "customers"
    }
  ]
}
```

MySQL/PostgreSQL pairs use ape-dts. A DuckDB target uses the SwanLake Flight SQL
adapter. DuckDB full loads materialize a source snapshot and replace all mapped
target tables in one target transaction.

### `GET /tasks/{id}`

Get task details including recent run history.

### `POST /tasks/{id}/start`

Manually trigger a task run.

### `POST /tasks/{id}/schedule`

Set or update the cron schedule for a task.

**Request:**
```json
{
  "cron_expr": "0 */6 * * *"
}
```

### `GET /tasks/{id}/schedule`

Get current schedule for a task.

### `DELETE /tasks/{id}/schedule`

Remove the schedule (task becomes manual-trigger only).

### `PATCH /tasks/{id}/status`

Update task status. Valid values: `draft`, `running`, `paused`, `completed`, `failed`, `cancelled`.

---

## Monitoring

### `GET /monitoring/overview`

Get dashboard overview for the current tenant.

**Response:**
```json
{
  "total_tasks": 10,
  "running_tasks": 3,
  "failed_tasks": 1,
  "total_datasources": 8,
  "healthy_datasources": 7,
  "today_synced_rows": 1500000,
  "today_synced_bytes": 268435456,
  "success_rate": 90.0,
  "recent_runs": []
}
```

The legacy `success_rate` field is `100 × (total_tasks - failed_tasks) / total_tasks`:
the share of configured tasks whose current status is not failed. It is not an
execution success rate calculated from run history. The API preserves its existing
zero-task value for compatibility; the UI displays no percentage when the denominator
is zero. Healthy data-source counts include only confirmed healthy sources.

---

## Alerts

### `GET /alerts`

List alert rules for the current tenant.

### `POST /alerts`

Create an alert rule.

**Request:**
```json
{
  "name": "High Error Rate",
  "metric_type": "error_rate",
  "operator": ">",
  "threshold": 10.0,
  "task_id": null,
  "notify_channels": "email"
}
```

### `PATCH /alerts/{id}/toggle`

Toggle alert rule enabled/disabled.

---

## AI

### `POST /ai/chat`

Send a message to the DeerFlow AI assistant. Returns SSE stream.

**Request:**
```json
{
  "message": "Why did task X fail?",
  "history": []
}
```

**Response:** Server-Sent Events stream with `text/event-stream` content type.

### `POST /ai/diagnose?task_id={id}`

AI-powered task failure diagnosis. Fetches task context, run history,
and error messages, then streams root cause analysis and fix suggestions.

**Request:**
```json
{}
```

The `task_id` is passed as a query parameter. The server fetches all
relevant context (task details, data source types, recent run history,
error messages) and sends it to DeerFlow for expert analysis.

**Response:** Server-Sent Events stream. The AI responds in a structured
format covering:
1. Root cause analysis
2. Impact assessment
3. Fix suggestions (prioritized)
4. Prevention measures

---

## System (Admin)

All `/system` endpoints require `tenant_admin` role.

### Users

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/system/users` | List users |
| `POST` | `/system/users` | Create user |
| `PUT` | `/system/users/{id}` | Update user |
| `POST` | `/system/users/{id}/reset-password` | Reset password |
| `DELETE` | `/system/users/{id}` | Soft-delete user |

### Roles

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/system/roles` | List roles |
| `POST` | `/system/roles` | Create role with permissions |
| `PUT` | `/system/roles/{id}` | Update role name/permissions |
| `DELETE` | `/system/roles/{id}` | Delete role |

### Tenant Settings

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/system/tenants` | List all tenants |
| `PUT` | `/system/tenants/{id}` | Update tenant name/status |

### Permissions

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/system/permissions` | List all permission codes |

---

## WebSocket

### `GET /ws/tasks/{task_id}`

Real-time task log and progress stream via WebSocket.

**Protocol:** JSON frames with `{ "type": "log" | "progress", ... }`.

---

## Health Check

### `GET /health`

No authentication required.

**Response:**
```json
{
  "status": "healthy",
  "service": "dts-platform",
  "version": "0.1.0"
}
```

---

## Error Responses

Application errors follow this format (framework request parsing errors may differ):

```json
{
  "code": "NOT_FOUND",
  "message": "Requested resource not found"
}
```

| HTTP Status | Meaning |
|-------------|---------|
| 400 | Bad request / validation error |
| 401 | Missing, invalid, expired or revoked session; inactive account/tenant |
| 403 | Insufficient permissions |
| 404 | Resource not found |
| 500 | Internal server error |
