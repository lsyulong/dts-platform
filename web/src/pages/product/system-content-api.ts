import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import type { ProductResource } from '../../product/resource-types';

export function useVisibleContent(kind: 'portal' | 'announcement') {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: ['visible-content', user?.tenant_id, user?.id, kind],
    queryFn: ({ signal }) =>
      client
        .get<{ data: ProductResource[] }>(`/product/content/${kind}`, {
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data.data),
    enabled: !!user && !!token,
  });
}
