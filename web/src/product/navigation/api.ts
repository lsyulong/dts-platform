import { useQuery } from '@tanstack/react-query';
import client from '../../api/client';
import { isNavigationSession, type NavigationSession } from './session';
import type {
  AssignNavigationRequest,
  NavigationDecisionResult,
  NavigationView,
  RevokeNavigationRequest,
} from './types';

export const navigationQueryKey = (session: NavigationSession) => ['navigation', session.key];

export function useNavigationQuery<T>(
  session: NavigationSession,
  path: string,
  params: Record<string, string | number>,
  enabled = true,
) {
  return useQuery({
    queryKey: [...navigationQueryKey(session), path, params],
    enabled: session.ready && enabled,
    staleTime: 30_000,
    retry: false,
    queryFn: async ({ signal }) => {
      const response = await client.get<T>(path, {
        params,
        signal,
        headers: { Authorization: `Bearer ${session.token}` },
      });
      if (!isNavigationSession(session)) throw new Error('Navigation session changed');
      return response.data;
    },
  });
}

export function validateNavigationView(policy: NavigationView, session: NavigationSession) {
  const safePath = (path: string) =>
    /^\/(?!\/)[^?#\\]*$/.test(path) &&
    !Array.from(path).some(
      (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    );
  if (
    policy.tenant_id !== session.tenantId ||
    policy.affects_navigation_only !== true ||
    !['default_catalog', 'assigned', 'expired'].includes(policy.mode) ||
    !Number.isSafeInteger(policy.assignment_version) ||
    policy.assignment_version < 0 ||
    !Array.isArray(policy.nodes) ||
    !policy.nodes.every((node) => typeof node.path === 'string' && safePath(node.path))
  )
    throw new Error('Invalid navigation policy response');
  return policy;
}

export async function assignNavigation(
  session: NavigationSession,
  templateId: string,
  body: AssignNavigationRequest,
) {
  if (!isNavigationSession(session) || !session.canManage)
    throw new Error('Navigation permission changed');
  return (
    await client.post<NavigationDecisionResult>(
      `/product/navigation/assign/${encodeURIComponent(templateId)}`,
      body,
      { headers: { Authorization: `Bearer ${session.token}` } },
    )
  ).data;
}

export async function revokeNavigation(session: NavigationSession, body: RevokeNavigationRequest) {
  if (!isNavigationSession(session) || !session.canManage)
    throw new Error('Navigation permission changed');
  return (
    await client.post<NavigationDecisionResult>('/product/navigation/revoke', body, {
      headers: { Authorization: `Bearer ${session.token}` },
    })
  ).data;
}
