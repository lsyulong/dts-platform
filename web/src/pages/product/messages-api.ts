import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { captureSession, isCurrentSession } from '../../lib/session';
import { useAuthStore } from '../../stores/auth';
import { selectionSessionScope } from '../../product/business-editors/option-sources';

export interface MessageDelivery {
  id: string;
  source_kind: 'system_message' | 'alert_policy';
  message_id: string;
  subject: string;
  body: string;
  status: string;
  delivered_at: string;
  read_at: string | null;
}

export function useMessageInbox(unreadOnly: boolean, offset: number) {
  const user = useAuthStore((state) => state.user);
  const token = useAuthStore((state) => state.token);
  return useQuery({
    queryKey: [
      'message-inbox',
      user?.tenant_id,
      user?.id,
      selectionSessionScope(token),
      JSON.stringify(user?.permissions ?? []),
      unreadOnly,
      offset,
    ],
    queryFn: ({ signal }) =>
      client
        .get<{ data: MessageDelivery[]; total: number }>('/product/messages/inbox', {
          params: { unread_only: unreadOnly, limit: 20, offset },
          signal,
          headers: { Authorization: `Bearer ${token}` },
        })
        .then((response) => response.data),
    enabled: !!user && !!token,
  });
}

export function useMarkMessageRead() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const session = captureSession();
      const response = await client.post<MessageDelivery>(
        `/product/messages/${encodeURIComponent(id)}/read`,
        {},
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      return { message: response.data, session };
    },
    onSuccess: ({ session }) => {
      if (isCurrentSession(session))
        void cache.invalidateQueries({
          queryKey: ['message-inbox', session.tenantId, session.userId],
        });
    },
  });
}
