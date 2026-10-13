import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { selectionSessionScope } from '../../product/business-editors/option-sources';
import { apiErrorMessage } from '../../lib/apiError';

export interface ServiceCredential {
  id: string;
  tenant_id: string;
  application_id: string;
  owner_id: string;
  version: number;
  created_by: string;
  created_at: string;
  expires_at: string;
  revoked_at: string | null;
  revoked_by: string | null;
}

export function useServicePage<T>(
  path: string,
  filters: Record<string, unknown>,
  enabled: boolean,
) {
  const user = useAuthStore((state) => state.user),
    token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'service-access',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      path,
      filters,
    ],
    queryFn: async ({ signal }) => {
      try {
        return (
          await client.get<T>(path, {
            params: filters,
            signal,
            headers: { Authorization: `Bearer ${token}` },
          })
        ).data;
      } catch (error) {
        throw new Error(apiErrorMessage(error, '服务数据加载失败'));
      }
    },
    enabled: !!user && !!token && enabled,
    refetchInterval: path.endsWith('/calls') ? 10000 : false,
  });
}
