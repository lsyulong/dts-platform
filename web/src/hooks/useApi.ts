import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from '../api/client';

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
  created_at: string;
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
  });
}

export function useTask(id: string | undefined) {
  return useQuery({
    queryKey: ['tasks', id],
    queryFn: () => client.get(`/tasks/${id}`).then((r) => r.data as TaskDetail),
    enabled: !!id,
  });
}

export function useUpdateTaskStatus() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: string }) =>
      client.patch(`/tasks/${id}/status`, { status }),
    onMutate: async ({ id, status }) => {
      await qc.cancelQueries({ queryKey: ['tasks'] });
      const previous = qc.getQueryData<Task[]>(['tasks']);
      qc.setQueryData<Task[]>(['tasks'], (old) =>
        old?.map((t) => (t.id === id ? { ...t, status } : t))
      );
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        qc.setQueryData(['tasks'], context.previous);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] });
    },
  });
}

export function useCreateTask() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: {
      name: string;
      description: string;
      source_id: string;
      target_id: string;
      cron_expr: string | null;
    }) => client.post('/tasks', body),
    onSuccess: () => {
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
    mutationFn: (body: Record<string, unknown>) => client.post('/datasources', body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['datasources'] });
    },
  });
}

export function useDeleteDatasource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => client.delete(`/datasources/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['datasources'] });
    },
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
      await qc.cancelQueries({ queryKey: ['alerts'] });
      const previous = qc.getQueryData<AlertRule[]>(['alerts']);
      qc.setQueryData<AlertRule[]>(['alerts'], (old) =>
        old?.map((a) => (a.id === id ? { ...a, enabled: !a.enabled } : a))
      );
      return { previous };
    },
    onError: (_err, _id, context) => {
      if (context?.previous) {
        qc.setQueryData(['alerts'], context.previous);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ['alerts'] });
    },
  });
}

// ── Scheduler ─────────────────────────────────────────────────────

export interface ScheduleStatus {
  has_timer: boolean;
  cron_expr: string | null;
  next_run: string | null;
  timer_name: string | null;
  schedule_history: ScheduleInstance[];
}

export interface ScheduleInstance {
  id: number;
  name: string;
  start_time: string | null;
  end_time: string | null;
  exit_code: number | null;
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
    mutationFn: ({ taskId, cronExpr }: { taskId: string; cronExpr: string }) =>
      client.post(`/tasks/${taskId}/schedule`, { cron_expr: cronExpr }),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['schedule', vars.taskId] });
    },
  });
}

export function useDeleteSchedule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => client.delete(`/tasks/${taskId}/schedule`),
    onSuccess: (_data, taskId) => {
      qc.invalidateQueries({ queryKey: ['schedule', taskId] });
    },
  });
}
