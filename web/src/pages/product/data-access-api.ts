import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import type { ResourcePage } from '../../product/resource-types';

export interface DataAccessAudit {
  id: string;
  actor_id: string;
  action: string;
  outcome: string;
  evidence: Record<string, unknown>;
  created_at: string;
}
export interface DataAccessGrant {
  id: string;
  request_id: string;
  metadata_id: string;
  metadata_version: number;
  table_catalog: string;
  schema_name: string;
  table_name: string;
  columns: string[];
  expires_at: string;
}

export function useDataAccessPage<T>(
  view: 'approvals' | 'audit' | 'grants',
  status: string,
  offset: number,
  enabled: boolean,
) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: ['data-access', user?.tenant_id, user?.id, view, status, offset],
    queryFn: ({ signal }) =>
      client
        .get<T>(`/product/data-access/${view}`, {
          signal,
          params: { status: status || undefined, limit: 20, offset },
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!user && !!token && enabled,
    refetchInterval: view === 'grants' ? 30_000 : false,
  });
}

export type ApprovalPage = ResourcePage;
export type AccessAuditPage = { data: DataAccessAudit[]; total: number };
