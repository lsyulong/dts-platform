import { http, HttpResponse } from 'msw';

const API_BASE = 'http://localhost:8080/api/v1';

// ── Auth ──────────────────────────────────────────────────────────
export const loginHandler = http.post(`${API_BASE}/auth/login`, async ({ request }) => {
  const body = (await request.json()) as { username: string; password: string; tenant_slug: string };
  if (body.username === 'admin' && body.password === 'admin123') {
    return HttpResponse.json({
      token: 'mock-jwt-token-abc123',
      user: {
        id: 'u1',
        username: 'admin',
        email: 'admin@dts.io',
        display_name: '管理员',
        tenant_id: 't1',
        tenant_name: 'default',
        roles: ['admin', 'operator'],
        permissions: ['task:read', 'task:write', 'ds:read', 'ds:write', 'monitor:read'],
      },
    });
  }
  return HttpResponse.json({ message: '用户名或密码错误' }, { status: 401 });
});

export const getMeHandler = http.get(`${API_BASE}/auth/me`, () => {
  return HttpResponse.json({
    id: 'u1',
    username: 'admin',
    email: 'admin@dts.io',
    display_name: '管理员',
    tenant_id: 't1',
    tenant_name: 'default',
    roles: ['admin'],
    permissions: ['task:read', 'task:write'],
  });
});

export const getPermissionsHandler = http.get(`${API_BASE}/auth/permissions`, () => {
  return HttpResponse.json([
    'task:read', 'task:write', 'task:delete',
    'ds:read', 'ds:write', 'ds:delete',
    'monitor:read', 'monitor:write',
  ]);
});

// ── Tasks ─────────────────────────────────────────────────────────
export const getTasksHandler = http.get(`${API_BASE}/tasks`, () => {
  return HttpResponse.json({
    data: [
      {
        id: 'task-1',
        name: 'MySQL to PostgreSQL Sync',
        description: 'Production data sync task',
        source_id: 'ds-1',
        target_id: 'ds-2',
        cron_expr: '0 */6 * * *',
        status: 'running',
        created_at: '2026-01-15T08:00:00Z',
      },
      {
        id: 'task-2',
        name: 'MongoDB to ClickHouse',
        description: null,
        source_id: 'ds-3',
        target_id: 'ds-4',
        cron_expr: null,
        status: 'draft',
        created_at: '2026-02-01T10:30:00Z',
      },
      {
        id: 'task-3',
        name: 'Kafka to Redis Cache',
        description: 'Real-time cache sync',
        source_id: 'ds-5',
        target_id: 'ds-6',
        cron_expr: '*/5 * * * *',
        status: 'paused',
        created_at: '2026-03-10T14:00:00Z',
      },
      {
        id: 'task-4',
        name: 'Oracle Migration',
        description: 'Legacy Oracle to PG migration',
        source_id: 'ds-7',
        target_id: 'ds-8',
        cron_expr: null,
        status: 'failed',
        created_at: '2026-04-20T09:00:00Z',
      },
      {
        id: 'task-5',
        name: 'Completed Backup Sync',
        description: 'Nightly backup sync',
        source_id: 'ds-9',
        target_id: 'ds-10',
        cron_expr: '0 2 * * *',
        status: 'completed',
        created_at: '2026-05-01T00:00:00Z',
      },
    ],
  });
});

export const createTaskHandler = http.post(`${API_BASE}/tasks`, async ({ request }) => {
  const body = await request.json();
  return HttpResponse.json({
    id: 'task-new',
    name: (body as { name: string }).name || 'New Task',
    status: 'draft',
    created_at: new Date().toISOString(),
  }, { status: 201 });
});

export const getTaskDetailHandler = http.get(`${API_BASE}/tasks/:id`, ({ params }) => {
  const { id } = params;
  return HttpResponse.json({
    id,
    name: 'MySQL to PostgreSQL Sync',
    description: 'Production data sync task',
    source_name: 'prod-mysql',
    source_type: 'MySQL',
    target_name: 'prod-pg',
    target_type: 'PostgreSQL',
    cron_expr: '0 */6 * * *',
    status: 'running',
    config_json: '{}',
    created_at: '2026-01-15T08:00:00Z',
    updated_at: '2026-06-01T12:00:00Z',
    recent_runs: [
      {
        id: 'run-1',
        status: 'running',
        start_time: '2026-07-05T10:00:00Z',
        end_time: null,
        rows_synced: 125000,
        bytes_synced: 524288000,
        total_rows: 500000,
        error_message: null,
      },
      {
        id: 'run-2',
        status: 'completed',
        start_time: '2026-07-04T22:00:00Z',
        end_time: '2026-07-04T23:30:00Z',
        rows_synced: 500000,
        bytes_synced: 2147483648,
        total_rows: 500000,
        error_message: null,
      },
      {
        id: 'run-3',
        status: 'failed',
        start_time: '2026-07-04T16:00:00Z',
        end_time: '2026-07-04T16:05:00Z',
        rows_synced: 0,
        bytes_synced: 0,
        total_rows: 500000,
        error_message: 'Connection timeout: unable to reach source database after 30s',
      },
    ],
  });
});

export const updateTaskStatusHandler = http.patch(`${API_BASE}/tasks/:id/status`, () => {
  return HttpResponse.json({ success: true, status: 'updated' });
});

// ── Datasources ───────────────────────────────────────────────────
export const getDatasourcesHandler = http.get(`${API_BASE}/datasources`, () => {
  return HttpResponse.json({
    data: [
      {
        id: 'ds-1',
        name: 'prod-mysql',
        db_type: 'MySQL',
        host: '10.0.1.100',
        port: 3306,
        username: 'admin',
        database_name: 'production',
        health_status: 'healthy',
        created_at: '2026-01-10T08:00:00Z',
      },
      {
        id: 'ds-2',
        name: 'prod-pg',
        db_type: 'PostgreSQL',
        host: '10.0.2.50',
        port: 5432,
        username: null,
        database_name: 'analytics',
        health_status: 'healthy',
        created_at: '2026-01-12T09:00:00Z',
      },
      {
        id: 'ds-3',
        name: 'staging-redis',
        db_type: 'Redis',
        host: '10.0.3.10',
        port: 6379,
        username: null,
        database_name: null,
        health_status: 'unhealthy',
        created_at: '2026-02-05T14:00:00Z',
      },
      {
        id: 'ds-4',
        name: 'mongo-logs',
        db_type: 'MongoDB',
        host: '10.0.4.20',
        port: 27017,
        username: 'logger',
        database_name: 'app_logs',
        health_status: 'healthy',
        created_at: '2026-03-01T11:00:00Z',
      },
    ],
  });
});

export const createDatasourceHandler = http.post(`${API_BASE}/datasources`, async ({ request }) => {
  const body = await request.json();
  return HttpResponse.json({
    id: 'ds-new',
    name: (body as { name: string }).name || 'New Datasource',
    db_type: (body as { db_type: string }).db_type || 'MySQL',
    host: (body as { host: string }).host || 'localhost',
    health_status: 'healthy',
    created_at: new Date().toISOString(),
  }, { status: 201 });
});

export const deleteDatasourceHandler = http.delete(`${API_BASE}/datasources/:id`, () => {
  return HttpResponse.json({ success: true });
});

// ── Monitoring ────────────────────────────────────────────────────
export const getOverviewHandler = http.get(`${API_BASE}/monitoring/overview`, () => {
  return HttpResponse.json({
    total_tasks: 12,
    running_tasks: 3,
    failed_tasks: 2,
    total_datasources: 8,
    healthy_datasources: 7,
    today_synced_rows: 2500000,
    today_synced_bytes: 10737418240,
    success_rate: 95.8,
    recent_runs: [
      {
        id: 'rr-1',
        task_id: 'task-1',
        task_name: 'MySQL to PG Sync',
        status: 'running',
        start_time: '2026-07-05 10:30',
        rows_synced: 500000,
        bytes_synced: 2147483648,
      },
      {
        id: 'rr-2',
        task_id: 'task-2',
        task_name: 'Mongo to CH',
        status: 'completed',
        start_time: '2026-07-05 08:00',
        rows_synced: 1200000,
        bytes_synced: 5368709120,
      },
      {
        id: 'rr-3',
        task_id: 'task-3',
        task_name: 'Kafka to Redis',
        status: 'failed',
        start_time: '2026-07-05 07:45',
        rows_synced: 30000,
        bytes_synced: 134217728,
      },
    ],
  });
});

// ── Alerts (mutable state for toggle) ───────────────────────────────
const alertsState = [
  {
    id: 'alert-1',
    name: '高延迟告警',
    metric_type: 'latency',
    operator: '>',
    threshold: 500,
    enabled: true,
    task_name: 'MySQL to PG Sync',
    created_at: '2026-01-15T08:00:00Z',
  },
  {
    id: 'alert-2',
    name: '错误率过高',
    metric_type: 'error_rate',
    operator: '>',
    threshold: 5,
    enabled: true,
    task_name: 'Mongo to CH',
    created_at: '2026-02-01T10:00:00Z',
  },
  {
    id: 'alert-3',
    name: '吞吐量过低',
    metric_type: 'throughput',
    operator: '<',
    threshold: 10,
    enabled: false,
    task_name: null,
    created_at: '2026-03-20T14:00:00Z',
  },
];

export const getAlertsHandler = http.get(`${API_BASE}/alerts`, () => {
  return HttpResponse.json({
    data: [...alertsState],
  });
});

export const createAlertHandler = http.post(`${API_BASE}/alerts`, async ({ request }) => {
  const body = await request.json() as { name: string };
  const newAlert = {
    id: 'alert-new',
    name: body.name || 'New Alert',
    metric_type: 'latency',
    operator: '>',
    threshold: 100,
    enabled: true,
    task_name: null,
    created_at: new Date().toISOString(),
  };
  alertsState.push(newAlert);
  return HttpResponse.json(newAlert, { status: 201 });
});

export const toggleAlertHandler = http.patch(`${API_BASE}/alerts/:id/toggle`, ({ params }) => {
  const { id } = params;
  const alert = alertsState.find((a) => a.id === id);
  if (alert) {
    alert.enabled = !alert.enabled;
  }
  return HttpResponse.json({ success: true, enabled: alert?.enabled ?? false });
});

// ── Aggregated export ─────────────────────────────────────────────
export const handlers = [
  // Auth
  loginHandler,
  getMeHandler,
  getPermissionsHandler,
  // Tasks
  getTasksHandler,
  createTaskHandler,
  getTaskDetailHandler,
  updateTaskStatusHandler,
  // Datasources
  getDatasourcesHandler,
  createDatasourceHandler,
  deleteDatasourceHandler,
  // Monitoring
  getOverviewHandler,
  getAlertsHandler,
  createAlertHandler,
  toggleAlertHandler,
];
