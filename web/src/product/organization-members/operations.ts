import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import client from '../../api/client';
import { apiErrorMessage } from '../../lib/apiError';
import { isNavigationSession } from '../navigation/session';
import {
  membershipPath,
  organizationPath,
  organizationQueryKey,
  type OrganizationSession,
} from './api';
import type { OrganizationDecisionResult, OrganizationIdentity } from './types';

type Common = {
  reason: string;
  source: OrganizationIdentity | null;
  userId: string | null;
  userName: string;
  isManager: boolean;
  handoverChoice: '' | 'successor' | 'resign';
  successorId: string;
};
export type PendingOrganizationOperation = Common &
  (
    | {
        kind: 'transfer';
        membershipVersion: number;
        target: OrganizationIdentity | null;
        targetId: string;
      }
    | { kind: 'remove'; membershipVersion: number }
    | { kind: 'manager'; managerUserId: string | null }
  );
export function organizationOperation(operation: PendingOrganizationOperation) {
  const reason = operation.reason.trim();
  if (!reason) throw new Error('请填写操作原因 / An operation reason is required');
  if (operation.kind === 'manager') {
    if (!operation.source) throw new Error('请重新读取实际组织 / Reload the actual organization');
    return {
      path: `${organizationPath(operation.source.id)}/manager`,
      body: {
        expected_organization_version: operation.source.version,
        manager_user_id: operation.managerUserId,
        reason,
      },
    };
  }
  if (operation.isManager && !operation.source)
    throw new Error('原组织信息已失效 / Original organization information is unavailable');
  if (operation.isManager && !operation.handoverChoice)
    throw new Error('请选择继任负责人或明确卸任 / Choose a successor or explicit resignation');
  if (operation.isManager && operation.handoverChoice === 'successor' && !operation.successorId)
    throw new Error(
      '请选择本组织有效成员作为继任负责人 / Choose an active member of this organization',
    );
  const manager_handover =
    operation.isManager && operation.source
      ? {
          expected_organization_version: operation.source.version,
          successor_user_id: operation.handoverChoice === 'resign' ? null : operation.successorId,
        }
      : null;
  const body = {
    expected_membership_version: operation.membershipVersion,
    reason,
    manager_handover,
  };
  if (operation.kind === 'remove')
    return { path: `${membershipPath(operation.userId ?? '')}/remove`, body };
  if (!operation.target || operation.target.id !== operation.targetId)
    throw new Error('请选择并读取实际目标组织 / Select and load the target organization');
  return {
    path: `${membershipPath(operation.userId ?? '')}/transfer`,
    body: {
      ...body,
      target_organization_id: operation.target.id,
      expected_target_version: operation.target.version,
    },
  };
}
export function useOrganizationOperations(session: OrganizationSession) {
  const cache = useQueryClient();
  const [pending, setPending] = useState<PendingOrganizationOperation | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [result, setResult] = useState<OrganizationDecisionResult | null>(null);
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
  const begin = (operation: PendingOrganizationOperation) => {
    generation.current++;
    setPending(operation);
    setError('');
    setResult(null);
  };
  const commit = async () => {
    if (!pending || busy || !session.canManage || !isNavigationSession(session)) return;
    let request: ReturnType<typeof organizationOperation>;
    try {
      request = organizationOperation(pending);
    } catch (caught) {
      setError(apiErrorMessage(caught, '操作信息不完整'));
      return;
    }
    const semantic = JSON.stringify(request),
      key = keys.current.get(semantic) ?? crypto.randomUUID();
    keys.current.set(semantic, key);
    const attempt = generation.current,
      current = () => mounted.current && isNavigationSession(session);
    setBusy(true);
    setError('');
    try {
      const response = await client.post<OrganizationDecisionResult>(
        request.path,
        { ...request.body, idempotency_key: key },
        { headers: { Authorization: `Bearer ${session.token}` } },
      );
      if (!current()) return;
      if (
        response.data.roles_unchanged !== true ||
        (response.data.membership && response.data.membership.tenant_id !== session.tenantId)
      )
        throw new Error('Invalid organization receipt');
      keys.current.delete(semantic);
      void cache.invalidateQueries({ queryKey: organizationQueryKey(session) });
      void cache.invalidateQueries({ queryKey: ['product-resources'] });
      if (attempt === generation.current) {
        setPending(null);
        setResult(response.data);
      }
    } catch (caught) {
      if (current() && attempt === generation.current)
        setError(apiErrorMessage(caught, '组织操作失败 / Organization operation failed'));
    } finally {
      if (current()) setBusy(false);
    }
  };
  return { pending, setPending, busy, error, result, begin, close, commit };
}
