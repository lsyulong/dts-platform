import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from '../api/client';
import type { TableMapping } from '../lib/validations';
import { captureSession, isCurrentSession } from '../lib/session';

// ── Types ──────────────────────────────────────────────────────────

export interface Task {
  id: string;
  name: string;
  description: string | null;
  source_id: string;
  target_id: string;
  cron_expr: string | null;
  status: string;
  created_at: string;
}

export interface TaskDetail {
  id: string;
  name: string;
  description: string | null;
  source_name: string;
  source_type: string;
  target_name: string;
  target_type: string;
  cron_expr: string | null;
  status: string;
  config_json: string;
  created_at: string;
  updated_at: string;
  recent_runs: Array<{
    id: string;
    status: string;
    start_time: string;
    end_time: string | null;
    rows_synced: number;
    bytes_synced: number;
    total_rows: number;
    error_message: string | null;
  }>;
}

export interface Datasource {
  id: string;
  name: string;
  db_type: string;
  host: string;
  port: number;
  username: string | null;
  database_name: string | null;
  health_status: string;
  last_test_at: string | null;
  created_at: string;
}

export interface DatasourceProbe {
  id: string;
  datasource_id: string;
  supported: boolean;
  status: 'healthy' | 'unhealthy' | 'unknown';
  latency_ms: number | null;
  error_message: string | null;
  actor_id: string;
  observed_at: string;
}

export interface DatasourceTable {
  schema: string;
  name: string;
}

export interface Overview {
  total_tasks: number;
  running_tasks: number;
  failed_tasks: number;
  total_datasources: number;
  healthy_datasources: number;
  today_synced_rows: number;
  today_synced_bytes: number;
  success_rate: number;
  recent_runs: Array<{
    id: string;
    task_id: string;
    task_name: string;
    status: string;
    start_time: string;
    rows_synced: number;
    bytes_synced: number;
  }>;
}

export interface AlertRule {
  id: string;
  name: string;
  metric_type: string;
  operator: string;
  threshold: number;
  enabled: boolean;
  task_name: string | null;
  created_at: string;
}

// ── Tasks ──────────────────────────────────────────────────────────

export function useTasks() {
  return useQuery({
    queryKey: ['tasks'],
    queryFn: () => client.get('/tasks').then((r) => r.data.data as Task[]),
    refetchInterval: 10000,
  });
}

export function useTask(id: string | undefined) {
  return useQuery({
    queryKey: ['tasks', id],
    queryFn: () => client.get(`/tasks/${id}`).then((r) => r.data as TaskDetail),
    enabled: !!id,
    refetchInterval: (query) => (query.state.data?.status === 'running' ? 5000 : false),
  });
}

export function useUpdateTaskStatus() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: ({ id, status }: { id: string; status: 'running' | 'cancelled' }) =>
      status === 'running'
        ? client.post(`/tasks/${id}/start`)
        : client.patch(`/tasks/${id}/status`, { status }),
    onSettled: (_data, _error, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['tasks'] });
    },
  });
}

export function useCreateTask() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: (body: {
      name: string;
      description: string;
      source_id: string;
      target_id: string;
      cron_expr: string | null;
      table_mappings: TableMapping[];
      sync_mode: 'full';
      batch_size?: number;
    }) => client.post('/tasks', body),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['tasks'] });
    },
  });
}

// ── Datasources ────────────────────────────────────────────────────

export function useDatasources() {
  return useQuery({
    queryKey: ['datasources'],
    queryFn: () => client.get('/datasources').then((r) => r.data.data as Datasource[]),
  });
}

export function useCreateDatasource() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: (body: Record<string, unknown>) => client.post('/datasources', body),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['datasources'] });
    },
  });
}

export function useDeleteDatasource() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: (id: string) => client.delete(`/datasources/${id}`),
    onSuccess: (_data, _vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['datasources'] });
    },
  });
}

export function useTestDatasource() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: (id: string) =>
      client.post(`/datasources/${id}/test`).then((response) => response.data as DatasourceProbe),
    onSuccess: (_data, id, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['datasources'] });
      qc.invalidateQueries({ queryKey: ['datasources', id, 'probes'] });
    },
  });
}

export function useDatasourceProbeHistory(id: string | undefined) {
  return useQuery({
    queryKey: ['datasources', id, 'probes'],
    queryFn: () =>
      client
        .get(`/datasources/${id}/probes`, { params: { limit: 20 } })
        .then((response) => response.data.data as DatasourceProbe[]),
    enabled: !!id,
  });
}

export function useDatasourceTables(id: string | undefined) {
  return useQuery({
    queryKey: ['datasources', id, 'tables'],
    queryFn: () =>
      client
        .get(`/datasources/${id}/tables`)
        .then((response) => response.data.data as DatasourceTable[]),
    enabled: !!id,
    staleTime: 30_000,
  });
}

// ── Monitoring ─────────────────────────────────────────────────────

export function useOverview() {
  return useQuery({
    queryKey: ['monitoring/overview'],
    queryFn: () => client.get('/monitoring/overview').then((r) => r.data as Overview),
  });
}

export function useAlerts() {
  return useQuery({
    queryKey: ['alerts'],
    queryFn: () => client.get('/alerts').then((r) => r.data.data as AlertRule[]),
  });
}

export function useToggleAlert() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => client.patch(`/alerts/${id}/toggle`),
    onMutate: async (id: string) => {
      const session = captureSession();
      await qc.cancelQueries({ queryKey: ['alerts'] });
      if (!isCurrentSession(session)) return { session, previous: undefined };
      const previous = qc.getQueryData<AlertRule[]>(['alerts']);
      qc.setQueryData<AlertRule[]>(['alerts'], (old) =>
        old?.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a)),
      );
      return { previous, session };
    },
    onError: (_err, _id, context) => {
      if (isCurrentSession(context?.session) && context?.previous) {
        qc.setQueryData(['alerts'], context.previous);
      }
    },
    onSettled: (_data, _error, _id, context) => {
      if (!isCurrentSession(context?.session)) return;
      qc.invalidateQueries({ queryKey: ['alerts'] });
    },
  });
}

// ── Scheduler ─────────────────────────────────────────────────────

export interface ScheduleStatus {
  enabled?: boolean;
  active?: boolean;
  state?: 'active' | 'pending' | 'offline' | 'stopped' | 'inactive' | 'disabled';
  timezone?: string;
  has_timer: boolean;
  cron_expr: string | null;
  next_run: string | null;
  timer_name: string | null;
  schedule_history: ScheduleInstance[];
}

export interface ScheduleInstance {
  id: number;
  name: string;
  created_time: string | null;
  status: string;
}

export function useScheduleStatus(taskId: string | undefined) {
  return useQuery({
    queryKey: ['schedule', taskId],
    queryFn: () => client.get(`/tasks/${taskId}/schedule`).then((r) => r.data as ScheduleStatus),
    enabled: !!taskId,
  });
}

export function useSetSchedule() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: ({ taskId, cronExpr }: { taskId: string; cronExpr: string }) =>
      client.post(`/tasks/${taskId}/schedule`, { cron_expr: cronExpr }),
    onSuccess: (_data, vars, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['schedule', vars.taskId] });
    },
  });
}

export function useDeleteSchedule() {
  const qc = useQueryClient();
  return useMutation({
    onMutate: captureSession,
    mutationFn: (taskId: string) => client.delete(`/tasks/${taskId}/schedule`),
    onSuccess: (_data, taskId, session) => {
      if (!isCurrentSession(session)) return;
      qc.invalidateQueries({ queryKey: ['schedule', taskId] });
    },
  });
}
