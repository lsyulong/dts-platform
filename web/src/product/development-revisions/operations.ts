import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { apiErrorMessage } from '../../lib/apiError';
import { isNavigationSession } from '../navigation/session';
import { checkSummary, revisionKey, revisionPath, type RevisionSession } from './api';
import type { RevisionKind, RestoreRevisionResult, SaveRevisionResult } from './types';

export type RevisionOperation = {
  action: 'save' | 'restore';
  version: number;
  revisionId: string | null;
  reason: string;
};
export function useRevisionOperations(session: RevisionSession, kind: RevisionKind, id: string) {
  const cache = useQueryClient();
  const [pending, setPending] = useState<RevisionOperation | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  const [result, setResult] = useState<SaveRevisionResult | RestoreRevisionResult | null>(null);
  const keys = useRef(new Map<string, string>()),
    generation = useRef(0),
    mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const close = () => {
    generation.current++;
    setPending(null);
    setError('');
  };
  const begin = (operation: RevisionOperation) => {
    generation.current++;
    setPending(operation);
    setError('');
    setResult(null);
  };
  const commit = async () => {
    if (!pending || busy || !session.canRead || !session.canWrite || !isNavigationSession(session))
      return;
    const reason = pending.reason.trim();
    if (!reason) {
      setError('请填写修订原因 / A revision reason is required');
      return;
    }
    const path =
      pending.action === 'save'
        ? `${revisionPath(kind, id)}/revisions`
        : `${revisionPath(kind, id)}/revisions/${encodeURIComponent(pending.revisionId ?? '')}/restore`;
    const body = { expected_version: pending.version, reason };
    const semantic = JSON.stringify([path, body]);
    const key = keys.current.get(semantic) ?? crypto.randomUUID();
    keys.current.set(semantic, key);
    const attempt = generation.current;
    const current = () => mounted.current && isNavigationSession(session);
    setBusy(true);
    setError('');
    try {
      const response = await client.post<SaveRevisionResult | RestoreRevisionResult>(
        path,
        { ...body, idempotency_key: key },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (!current()) return;
      checkSummary(response.data.revision, session.tenantId ?? '', kind, id);
      if (
        'resource' in response.data &&
        (response.data.resource.tenant_id !== session.tenantId ||
          response.data.resource.kind !== kind ||
          response.data.resource.id !== id ||
          response.data.resource.status !== 'draft' ||
          response.data.requires_publish !== true)
      )
        throw new Error('Invalid restore receipt');
      keys.current.delete(semantic);
      void cache.invalidateQueries({ queryKey: revisionKey(session) });
      void cache.invalidateQueries({ queryKey: ['product-resources'] });
      if (attempt === generation.current) {
        setPending(null);
        setResult(response.data);
      }
    } catch (caught) {
      if (current() && attempt === generation.current)
        setError(apiErrorMessage(caught, '修订操作失败 / Revision operation failed'));
    } finally {
      if (current()) setBusy(false);
    }
  };
  return { pending, setPending, busy, error, result, begin, close, commit };
}
