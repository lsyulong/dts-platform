import { http, HttpResponse } from 'msw';
import { useAuthStore } from '../../../stores/auth';
import { server } from '../../../test/mocks/server';
import {
  workSources,
  type ProjectionBucket,
  type ProjectionCount,
  type WorkItem,
} from '../platform-projection-api';
export const API = 'http://localhost:8080/api/v1';
export const actor = {
  id: 'reader',
  tenant_id: 'tenant',
  tenant_name: 'Tenant',
  username: 'reader',
  email: 'reader@example.invalid',
  roles: ['reader'],
  permissions: [
    'system:read',
    'development:read',
    'development:execute',
    'data_security:read',
    'services:read',
  ],
};
export function login(permissions = actor.permissions, token = 'projection-session') {
  const user = { ...actor, permissions };
  localStorage.setItem('user', JSON.stringify(user));
  localStorage.setItem('token', token);
  useAuthStore.setState({ user, token, initialized: true, restoring: false, restoreError: null });
}
export function observation(
  source: string,
  total: number | null = 0,
  state: 'known' | 'unknown' | 'restricted' = 'known',
) {
  return {
    source,
    source_table: `actual_${source}`,
    observed_at: '2026-10-09T10:00:00Z',
    state,
    total,
    filter: 'tenant=current; actor=current; persisted rows',
    error_code: state === 'known' ? null : 'source_read_failed',
  };
}
export function item(id = 'actual-delivery'): WorkItem {
  return {
    id,
    source_kind: 'system_message',
    source_id: 'actual-message',
    source_version: 2,
    title: '实际待办消息',
    status: 'unread',
    created_at: '2026-10-09T10:00:00Z',
    due_at: null,
    route: '/system/messages',
    required_permissions: [],
  };
}
export function workResponse(
  request: Request,
  changed: Record<string, ProjectionBucket<WorkItem>> = {},
) {
  const url = new URL(request.url),
    source = url.searchParams.get('source'),
    offset = Number(url.searchParams.get('offset') ?? 0);
  return {
    tenant_id: actor.tenant_id,
    actor_id: actor.id,
    limit: 20,
    offset,
    sources: workSources
      .filter((s) => !source || s === source)
      .map((s) => changed[s] ?? { observation: observation(s), data: [] }),
  };
}
export function workHandler(changed: Record<string, ProjectionBucket<WorkItem>> = {}) {
  server.use(
    http.get(`${API}/product/work-items`, ({ request }) =>
      HttpResponse.json(workResponse(request, changed)),
    ),
  );
}
export const systemSources = [
  'users',
  'datasources',
  'sync_definitions',
  'product_definitions',
  'sync_runs',
  'product_jobs',
];
export const usageSources = [
  'login_events',
  'resource_operations',
  'query_access',
  'service_calls',
  'inbox',
];
export function statisticsResponse(
  request: Request,
  changed: Record<string, ProjectionBucket<ProjectionCount>> = {},
) {
  const url = new URL(request.url),
    view = url.searchParams.get('view');
  return {
    tenant_id: actor.tenant_id,
    actor_id: actor.id,
    view,
    start: url.searchParams.get('start'),
    end: url.searchParams.get('end'),
    sources: (view === 'system' ? systemSources : usageSources).map(
      (s) => changed[s] ?? { observation: observation(s), data: [] },
    ),
  };
}
export function statisticsHandler(changed: Record<string, ProjectionBucket<ProjectionCount>> = {}) {
  server.use(
    http.get(`${API}/product/statistics`, ({ request }) =>
      HttpResponse.json(statisticsResponse(request, changed)),
    ),
  );
}
