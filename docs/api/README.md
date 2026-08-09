# DTS Platform API Reference

Base URL: `/api/v1`

## Authentication

All endpoints except `/auth/login` and `/health` require a JWT token.

```
Authorization: Bearer <token>
```

The token is obtained via `POST /api/v1/auth/login` and expires after the configured duration.

---

## Auth

### `POST /auth/login`

Login with tenant slug, username, and password.

**Request:**
```json
{
  "tenant": "default",
  "username": "admin",
  "password": "admin123"
}
```

**Response (200):**
```json
{
  "token": "eyJ...",
  "user": {
    "id": "...",
    "username": "admin",
    "display_name": "Admin",
    "tenant_id": "...",
    "tenant_name": "Default",
    "roles": ["tenant_admin"],
    "permissions": ["*"]
  }
}
```

### `GET /auth/me`

Get current user info from JWT token. Requires `Authorization` header.

### `GET /auth/permissions`

Get current user's permission list.

---

## Tenants

### `POST /tenants`

Create a tenant. Requires `tenant_admin` role.

**Request:**
```json
{
  "name": "My Tenant",
  "slug": "my-tenant"
}
```

### `GET /tenants`

List all tenants.

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
  "name": "MySQL to PG Sync",
  "description": "Hourly sync of user data",
  "source_id": "<datasource-id>",
  "target_id": "<datasource-id>",
  "cron_expr": "0 * * * *",
  "config_json": "{}"
}
```

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
  "total_tasks": 12,
  "running_tasks": 3,
  "failed_tasks": 1,
  "total_datasources": 8,
  "healthy_datasources": 7,
  "today_synced_rows": 1500000,
  "today_synced_bytes": 268435456,
  "success_rate": 97.5,
  "recent_runs": [...]
}
```

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

All errors follow a consistent format:

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Requested resource not found"
  }
}
```

| HTTP Status | Meaning |
|-------------|---------|
| 400 | Bad request / validation error |
| 401 | Missing or invalid JWT token |
| 403 | Insufficient permissions |
| 404 | Resource not found |
| 409 | Conflict (duplicate slug, constraint violation) |
| 500 | Internal server error |
