import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { apiErrorMessage } from '../../lib/apiError';
import { assignNavigation, navigationQueryKey, revokeNavigation } from './api';
import { isNavigationSession, type NavigationSession } from './session';
import type { NavigationDecisionResult } from './types';

export type PendingNavigationOperation =
  | {
      kind: 'assign';
      templateId: string;
      templateName: string;
      templateVersion: number;
      assignmentVersion: number;
      validityDays: number | null;
      expiresAt: string | undefined;
    }
  | { kind: 'revoke'; assignmentId: string; assignmentVersion: number; reason: string };

export function useNavigationOperations(session: NavigationSession) {
  const cache = useQueryClient();
  const [pending, setPending] = useState<PendingNavigationOperation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [result, setResult] = useState<NavigationDecisionResult | null>(null);
  const keys = useRef(new Map<string, string>());
  const generation = useRef(0);
  const mounted = useRef(true);
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
  const begin = (operation: PendingNavigationOperation) => {
    generation.current++;
    setPending(operation);
    setResult(null);
    setError('');
  };
  const commit = async () => {
    if (!pending || busy || !session.canManage || !isNavigationSession(session)) return;
    if (pending.kind === 'revoke' && !pending.reason.trim()) {
      setError('请填写撤销原因 / A revocation reason is required');
      return;
    }
    const operation = pending;
    const attempt = generation.current;
    const payload =
      operation.kind === 'assign'
        ? {
            expected_template_version: operation.templateVersion,
            expected_assignment_version: operation.assignmentVersion,
            expires_at: operation.expiresAt ?? null,
          }
        : {
            assignment_id: operation.assignmentId,
            expected_assignment_version: operation.assignmentVersion,
            reason: operation.reason.trim(),
          };
    const semantic = JSON.stringify([
      operation.kind,
      operation.kind === 'assign' ? operation.templateId : null,
      payload,
    ]);
    const key = keys.current.get(semantic) ?? crypto.randomUUID();
    keys.current.set(semantic, key);
    const current = () => mounted.current && isNavigationSession(session);
    setBusy(true);
    setError('');
    try {
      const response =
        operation.kind === 'assign'
          ? await assignNavigation(session, operation.templateId, {
              expected_template_version: operation.templateVersion,
              expected_assignment_version: operation.assignmentVersion,
              expires_at: operation.expiresAt ?? null,
              idempotency_key: key,
            })
          : await revokeNavigation(session, {
              assignment_id: operation.assignmentId,
              expected_assignment_version: operation.assignmentVersion,
              reason: operation.reason.trim(),
              idempotency_key: key,
            });
      if (!current()) return;
      if (response.assignment.tenant_id !== session.tenantId)
        throw new Error('Invalid navigation receipt');
      keys.current.delete(semantic);
      void cache.invalidateQueries({ queryKey: navigationQueryKey(session) });
      if (generation.current === attempt) {
        setPending(null);
        setResult(response);
      }
    } catch (caught) {
      if (current() && generation.current === attempt)
        setError(apiErrorMessage(caught, '导航策略操作失败 / Navigation operation failed'));
    } finally {
      if (current()) setBusy(false);
    }
  };
  return { pending, setPending, busy, error, result, begin, close, commit };
}
