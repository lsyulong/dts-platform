import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { selectionSessionScope } from '../../product/business-editors/option-sources';
import type { ProductResource, ResourceActionResult } from '../../product/resource-types';
import type { ProductJob } from './jobs-api';

export interface SensitiveColumnEvidence {
  column_name: string;
  data_type: string;
  checked_values: number;
  matched_values: number;
  null_values: number;
}
export interface SensitiveFinding {
  id: string;
  tenant_id: string;
  task_id: string;
  task_version: number;
  job_id: string;
  node_id: string;
  object_id: string;
  object_version: number;
  rule_id: string;
  rule_version: number;
  classification_id: string;
  classification_version: number;
  column_name: string;
  evidence: SensitiveColumnEvidence;
  evaluation_scope: 'sample' | 'column_names';
  sampled_rows: number;
  definition_hash: string;
  status: 'pending' | 'confirmed' | 'dismissed';
  version: number;
  reviewed_by: string | null;
  review_reason: string | null;
  created_at: string;
  reviewed_at: string | null;
}
export interface SensitiveReviewReply {
  id: string;
  status: 'confirmed' | 'dismissed';
  version: number;
  reviewed_by: string;
  review_reason: string;
  reviewed_at: string;
  classification_enforced: false;
}
function useScope() {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return {
    user,
    token,
    key: [
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
    ],
    enabled: !!user && !!token && user.permissions.includes('sensitive_data:read'),
  };
}
export function useSensitiveFindings(taskId: string | undefined, status: string, offset: number) {
  const scope = useScope();
  return useQuery({
    queryKey: ['sensitive-findings', ...scope.key, taskId, status, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: SensitiveFinding[]; total: number }>('/product/sensitive-findings', {
          params: { task_id: taskId || undefined, status: status || undefined, limit: 20, offset },
          signal,
          headers: { Authorization: `Bearer ${scope.token}` },
        })
        .then((r) => r.data),
    enabled: scope.enabled,
    refetchInterval: 30_000,
  });
}
export function useSensitiveFinding(id: string | undefined) {
  const scope = useScope();
  return useQuery({
    queryKey: ['sensitive-finding', ...scope.key, id],
    queryFn: ({ signal }) =>
      client
        .get<SensitiveFinding>(`/product/sensitive-findings/${encodeURIComponent(id ?? '')}`, {
          signal,
          headers: { Authorization: `Bearer ${scope.token}` },
        })
        .then((r) => r.data),
    enabled: scope.enabled && !!id,
    refetchInterval: 15_000,
  });
}
export function useSensitiveTasks() {
  const scope = useScope();
  return useQuery({
    queryKey: ['sensitive-tasks', ...scope.key],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ProductResource[]; total: number }>('/product/resources/recognition_task', {
          params: { limit: 100, offset: 0 },
          signal,
          headers: { Authorization: `Bearer ${scope.token}` },
        })
        .then((r) => r.data),
    enabled: scope.enabled,
  });
}
export function useSensitiveJobs(taskId: string, status: string, offset: number) {
  const scope = useScope();
  return useQuery({
    queryKey: ['sensitive-jobs', ...scope.key, taskId, status, offset],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ProductJob[]; total: number }>('/product/jobs', {
          params: {
            source_kind: 'recognition_task',
            source_id: taskId || undefined,
            status: status || undefined,
            limit: 20,
            offset,
          },
          signal,
          headers: { Authorization: `Bearer ${scope.token}` },
        })
        .then((r) => r.data),
    enabled: scope.enabled,
    refetchInterval: (query) =>
      query.state.data?.data.some((job) => ['queued', 'running'].includes(job.status))
        ? 5_000
        : false,
  });
}
export function useSensitiveJob(id: string | undefined) {
  const scope = useScope();
  return useQuery({
    queryKey: ['sensitive-job', ...scope.key, id],
    queryFn: ({ signal }) =>
      client
        .get<ProductJob>(`/product/jobs/${encodeURIComponent(id ?? '')}`, {
          signal,
          headers: { Authorization: `Bearer ${scope.token}` },
        })
        .then((r) => r.data),
    enabled: scope.enabled && !!id,
    refetchInterval: (query) =>
      ['queued', 'running'].includes(query.state.data?.status ?? '') ? 5_000 : 15_000,
  });
}
export function useSensitiveReview() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: async (request: {
      id: string;
      action: 'confirm' | 'dismiss';
      expected_version: number;
      reason: string;
    }) => {
      const session = captureSession();
      const response = await client.post<SensitiveReviewReply>(
        `/product/sensitive-findings/${encodeURIComponent(request.id)}/${request.action}`,
        {
          expected_version: request.expected_version,
          reason: request.reason,
        },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      return { reply: response.data, session };
    },
    onSuccess: ({ session }) => {
      if (!isCurrentSession(session)) return;
      for (const key of ['sensitive-findings', 'sensitive-finding', 'product-resources'])
        void cache.invalidateQueries({ queryKey: [key] });
    },
  });
}
export function useSensitiveAction() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: async (
      request:
        | {
            task: ProductResource;
            action: 'validate' | 'publish' | 'withdraw' | 'execute';
            key?: string;
          }
        | { job: ProductJob; action: 'cancel' | 'retry'; key?: string },
    ) => {
      const session = captureSession(),
        headers = { Authorization: `Bearer ${session.token}` };
      const response =
        'task' in request
          ? await client.post<ResourceActionResult>(
              `/product/resources/recognition_task/${encodeURIComponent(request.task.id)}/actions/${request.action}`,
              {
                expected_version: request.task.version,
                payload: request.action === 'execute' ? { idempotency_key: request.key } : {},
              },
              { headers },
            )
          : await client.post<{ job: ProductJob }>(
              `/product/jobs/${encodeURIComponent(request.job.id)}/${request.action}`,
              request.action === 'retry' ? { idempotency_key: request.key } : {},
              { headers },
            );
      return { value: response.data, session };
    },
    onSuccess: ({ session }) => {
      if (!isCurrentSession(session)) return;
      for (const key of ['sensitive-tasks', 'sensitive-jobs', 'sensitive-job', 'product-resources'])
        void cache.invalidateQueries({ queryKey: [key] });
    },
  });
}
