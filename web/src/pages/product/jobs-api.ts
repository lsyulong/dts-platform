import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { captureSession, isCurrentSession } from '../../lib/session';
import { selectionSessionScope } from '../../product/business-editors/option-sources';

export interface ProductJobNode {
  id: string;
  name: string;
  status: string;
  depends_on: string[];
  attempt: number;
  result: Record<string, unknown> | null;
  error: string | null;
  result_access?: 'authorized' | 'restricted' | 'summary' | 'not_available';
  result_access_reason?: string | null;
}
export interface ProductJob {
  id: string;
  source_kind: string;
  source_id: string;
  source_version: number;
  status: string;
  cancel_requested: boolean;
  can_retry: boolean;
  attempt: number;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  nodes: ProductJobNode[];
  error: string | null;
  result_access?: 'authorized' | 'restricted' | 'summary' | 'not_available';
  result_access_reason?: string | null;
}

export function useProductJobs(
  sourceKind: string,
  status: string,
  offset: number,
  enabled: boolean,
) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-jobs',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      sourceKind,
      status,
      offset,
    ],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ProductJob[]; total: number }>('/product/jobs', {
          params: { source_kind: sourceKind, status: status || undefined, limit: 20, offset },
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!user && !!token && enabled,
    refetchInterval: (query) =>
      query.state.data?.data.some((job) => ['queued', 'running'].includes(job.status))
        ? 5000
        : false,
  });
}

export function useProductJobAction() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      action,
      key,
    }: {
      id: string;
      action: 'cancel' | 'retry';
      key: string;
    }) => {
      const session = captureSession();
      const response = await client.post(
        `/product/jobs/${encodeURIComponent(id)}/${action}`,
        action === 'retry' ? { idempotency_key: key } : {},
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      return { data: response.data, session };
    },
    onSuccess: ({ session }) => {
      if (isCurrentSession(session))
        void cache.invalidateQueries({
          queryKey: ['product-jobs', session.tenantId, session.userId],
        });
    },
  });
}

export function useProductJob(id: string | undefined, enabled: boolean) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'product-jobs',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      'detail',
      id,
    ],
    queryFn: ({ signal }) =>
      client
        .get<ProductJob>(`/product/jobs/${encodeURIComponent(id ?? '')}`, {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!user && !!token && !!id && enabled,
    refetchInterval: (query) =>
      ['queued', 'running'].includes(query.state.data?.status ?? '') ? 5000 : false,
  });
}
