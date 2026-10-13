import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { useAuthStore } from '../../stores/auth';
import { isNavigationSession, useNavigationSession } from '../navigation/session';
import type { ProductResource } from '../resource-types';
import type { Revision, RevisionDiff, RevisionKind, RevisionPage, RevisionSummary } from './types';

export function useRevisionSession() {
  const session = useNavigationSession();
  const permissions = useAuthStore((state) => state.user?.permissions);
  return {
    ...session,
    canRead: !!permissions?.includes('development:read'),
    canWrite: !!permissions?.includes('development:write'),
  };
}
export type RevisionSession = ReturnType<typeof useRevisionSession>;
export const revisionPath = (kind: RevisionKind, id: string) =>
  `/product/development/${kind}/${encodeURIComponent(id)}`;
export const revisionKey = (session: RevisionSession) => ['development-revisions', session.key];
export function checkSummary(r: RevisionSummary, tenant: string, kind: RevisionKind, id: string) {
  if (
    r.tenant_id !== tenant ||
    r.source_kind !== kind ||
    r.source_id !== id ||
    r.source_version < 1
  )
    throw new Error('Invalid revision identity');
}
export function useRevisionQuery<T>(
  path: string,
  params: Record<string, string | number>,
  enabled = true,
  validate?: (data: T, tenant: string) => void,
) {
  const session = useRevisionSession();
  return useQuery({
    queryKey: [...revisionKey(session), path, params],
    enabled: session.ready && session.canRead && enabled,
    staleTime: 0,
    retry: false,
    queryFn: async ({ signal }) => {
      const response = await client.get<T>(path, {
        params,
        signal,
        headers: { Authorization: `Bearer ${session.token}` },
      });
      if (!isNavigationSession(session)) throw new Error('Revision session changed');
      validate?.(response.data, session.tenantId ?? '');
      return response.data;
    },
  });
}
export function useRevisionCurrent(kind: RevisionKind, id: string) {
  return useRevisionQuery<ProductResource>(
    `/product/resources/${kind}/${encodeURIComponent(id)}`,
    {},
    !!id,
    (r, tenant) => {
      if (r.tenant_id !== tenant || r.kind !== kind || r.id !== id)
        throw new Error('Invalid development source');
    },
  );
}
export function useRevisionHistory(kind: RevisionKind, id: string, offset: number) {
  return useRevisionQuery<RevisionPage>(
    `${revisionPath(kind, id)}/revisions`,
    { limit: 20, offset },
    !!id,
    (page, tenant) => page.data.forEach((r) => checkSummary(r, tenant, kind, id)),
  );
}
export function useRevisionDetail(kind: RevisionKind, id: string, revisionId: string) {
  return useRevisionQuery<Revision>(
    `${revisionPath(kind, id)}/revisions/${encodeURIComponent(revisionId)}`,
    {},
    !!revisionId,
    (r, tenant) => {
      checkSummary(r, tenant, kind, id);
      if (
        r.id !== revisionId ||
        r.snapshot.id !== id ||
        r.snapshot.tenant_id !== tenant ||
        r.snapshot.kind !== kind ||
        r.snapshot.version !== r.source_version
      )
        throw new Error('Invalid revision snapshot');
    },
  );
}
export function useRevisionDiff(kind: RevisionKind, id: string, from: string, to: string) {
  return useRevisionQuery<RevisionDiff>(
    `${revisionPath(kind, id)}/diff`,
    { from_revision_id: from, ...(to ? { to_revision_id: to } : {}) },
    !!from,
    (r, tenant) => {
      if (
        r.tenant_id !== tenant ||
        r.source_kind !== kind ||
        r.source_id !== id ||
        r.from_revision_id !== from ||
        r.to_revision_id !== (to || null)
      )
        throw new Error('Invalid revision comparison');
    },
  );
}
