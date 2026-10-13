import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { selectionSessionScope } from '../../product/business-editors/option-sources';

export const workSources = [
  'data_access_approvals',
  'master_data_approvals',
  'service_approvals',
  'queued_jobs',
  'reconciliation_jobs',
  'unread_messages',
] as const;
export type WorkSource = (typeof workSources)[number];
export type StatisticsView = 'system' | 'usage';
export interface ProjectionObservation {
  source: string;
  source_table: string;
  observed_at: string;
  state: 'known' | 'unknown' | 'restricted';
  total: number | null;
  filter: string;
  error_code: string | null;
}
export interface WorkItem {
  id: string;
  source_kind: string;
  source_id: string;
  source_version: number | null;
  title: string;
  status: string;
  created_at: string;
  due_at: string | null;
  route: string | null;
  required_permissions: string[];
}
export interface ProjectionCount {
  dimension: string;
  count: number;
}
export interface ProjectionBucket<T> {
  observation: ProjectionObservation;
  data: T[];
}
export interface WorkItemsProjection {
  tenant_id: string;
  actor_id: string;
  limit: number;
  offset: number;
  sources: ProjectionBucket<WorkItem>[];
}
export interface StatisticsProjection {
  tenant_id: string;
  actor_id: string;
  view: StatisticsView;
  start: string;
  end: string;
  sources: ProjectionBucket<ProjectionCount>[];
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function integer(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function date(value: unknown): value is string {
  return typeof value === 'string' && Number.isFinite(Date.parse(value));
}
function observation(value: unknown): value is ProjectionObservation {
  return (
    record(value) &&
    typeof value.source === 'string' &&
    typeof value.source_table === 'string' &&
    date(value.observed_at) &&
    typeof value.filter === 'string' &&
    (value.error_code === null || typeof value.error_code === 'string') &&
    ((value.state === 'known' && integer(value.total) && value.error_code === null) ||
      ((value.state === 'unknown' || value.state === 'restricted') && value.total === null))
  );
}
function item(value: unknown): value is WorkItem {
  return (
    record(value) &&
    ['id', 'source_kind', 'source_id', 'title', 'status'].every(
      (k) => typeof value[k] === 'string',
    ) &&
    (value.source_version === null || integer(value.source_version)) &&
    date(value.created_at) &&
    (value.due_at === null || date(value.due_at)) &&
    (value.route === null ||
      (typeof value.route === 'string' &&
        value.route.startsWith('/') &&
        !value.route.startsWith('//') &&
        !value.route.includes('\\'))) &&
    Array.isArray(value.required_permissions) &&
    value.required_permissions.every((v) => typeof v === 'string')
  );
}
function count(value: unknown): value is ProjectionCount {
  return record(value) && typeof value.dimension === 'string' && integer(value.count);
}
function buckets<T>(
  value: unknown,
  valid: (entry: unknown) => entry is T,
): value is ProjectionBucket<T>[] {
  if (
    !Array.isArray(value) ||
    !value.every(
      (v) =>
        record(v) &&
        observation(v.observation) &&
        Array.isArray(v.data) &&
        v.data.every(valid) &&
        (v.observation.state === 'known' || v.data.length === 0),
    )
  )
    return false;
  return new Set(value.map((v) => v.observation.source)).size === value.length;
}
function identity(
  value: unknown,
  tenant: string | undefined,
  actor: string | undefined,
): value is Record<string, unknown> {
  return record(value) && value.tenant_id === tenant && value.actor_id === actor;
}
function useScope() {
  const user = useAuthStore((s) => s.user),
    token = useAuthStore((s) => s.token);
  return {
    user,
    token,
    key: [
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
    ],
  };
}
export function useWorkItems(source: WorkSource | undefined, offset: number) {
  const current = useScope();
  return useQuery({
    queryKey: ['platform-work-items', ...current.key, source, offset],
    enabled: !!current.user && !!current.token,
    retry: false,
    refetchInterval: 30_000,
    queryFn: async ({ signal }) => {
      const session = captureSession();
      const reply = await client.get<unknown>('/product/work-items', {
        params: { source, limit: 20, offset },
        signal,
        headers: { Authorization: `Bearer ${current.token}` },
      });
      if (!isCurrentSession(session)) throw new Error('会话已变更 / Session changed');
      const v = reply.data;
      if (
        !current.user ||
        !identity(v, current.user.tenant_id, current.user.id) ||
        v.limit !== 20 ||
        v.offset !== offset ||
        !buckets(v.sources, item) ||
        v.sources.some((s) => !workSources.some((x) => x === s.observation.source)) ||
        v.sources.some(
          (s) =>
            s.data.length > 20 ||
            (s.observation.total !== null && s.data.length > s.observation.total),
        ) ||
        (!source && v.sources.length !== workSources.length) ||
        (source && (v.sources.length !== 1 || v.sources[0]?.observation.source !== source))
      )
        throw new Error('待办投影格式不正确 / Invalid work-item projection');
      return {
        tenant_id: current.user.tenant_id,
        actor_id: current.user.id,
        limit: 20,
        offset,
        sources: v.sources,
      };
    },
  });
}
export function usePlatformStatistics(view: StatisticsView, start: string, end: string) {
  const current = useScope(),
    canRead = view === 'usage' || current.user?.permissions.includes('system:read');
  return useQuery({
    queryKey: ['platform-statistics', ...current.key, view, start, end],
    enabled: !!current.user && !!current.token && !!canRead && !!start && !!end,
    retry: false,
    queryFn: async ({ signal }) => {
      const session = captureSession();
      const reply = await client.get<unknown>('/product/statistics', {
        params: { view, start, end },
        signal,
        headers: { Authorization: `Bearer ${current.token}` },
      });
      if (!isCurrentSession(session)) throw new Error('会话已变更 / Session changed');
      const v = reply.data;
      const expected =
        view === 'system'
          ? [
              'users',
              'datasources',
              'sync_definitions',
              'product_definitions',
              'sync_runs',
              'product_jobs',
            ]
          : ['login_events', 'resource_operations', 'query_access', 'service_calls', 'inbox'];
      if (
        !current.user ||
        !identity(v, current.user.tenant_id, current.user.id) ||
        v.view !== view ||
        !date(v.start) ||
        !date(v.end) ||
        Date.parse(v.start) !== Date.parse(start) ||
        Date.parse(v.end) !== Date.parse(end) ||
        !buckets(v.sources, count) ||
        v.sources.length !== expected.length ||
        v.sources.some((s) => !expected.includes(s.observation.source)) ||
        v.sources.some(
          (s) =>
            s.observation.state === 'known' &&
            s.data.reduce((sum, row) => sum + row.count, 0) !== s.observation.total,
        )
      )
        throw new Error('统计投影格式不正确 / Invalid statistics projection');
      return {
        tenant_id: current.user.tenant_id,
        actor_id: current.user.id,
        view,
        start: v.start,
        end: v.end,
        sources: v.sources,
      };
    },
  });
}
