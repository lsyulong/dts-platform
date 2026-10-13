import { useContext, useEffect, useState } from 'react';
import { useNavigationQuery, validateNavigationView } from './api';
import { useNavigationSession } from './session';
import type { NavigationNode, NavigationView } from './types';
import { NavigationContext } from './context';

export interface NavigationPolicyState {
  phase: 'unauthenticated' | 'loading' | 'ready' | 'error';
  policy: NavigationView | null;
  nodes: NavigationNode[];
  error: unknown;
  allowsPath: (path: string) => boolean;
  refresh: () => Promise<void>;
}

export function useNavigationPolicy(): NavigationPolicyState {
  const session = useNavigationSession();
  const query = useNavigationQuery<NavigationView>(session, '/product/navigation/current', {});
  const [expiredResponse, setExpiredResponse] = useState<NavigationView | null>(null);
  const { data, dataUpdatedAt, refetch } = query;
  useEffect(() => {
    if (!data?.expires_at || data.mode !== 'assigned') return;
    const delay =
      Date.parse(data.expires_at) -
      Date.parse(data.observed_at) -
      Math.max(0, Date.now() - dataUpdatedAt);
    if (!Number.isFinite(delay)) return;
    const timer = window.setTimeout(
      () => {
        setExpiredResponse(data);
        void refetch();
      },
      Math.max(0, Math.min(delay, 2_147_483_647)),
    );
    return () => window.clearTimeout(timer);
  }, [data, dataUpdatedAt, refetch]);

  let policy: NavigationView | null = null;
  let error: unknown = session.error ?? query.error;
  if (session.ready && data && !query.isError && data !== expiredResponse) {
    try {
      policy = validateNavigationView(data, session);
    } catch (caught) {
      error = caught;
    }
  }
  const phase = error
    ? 'error'
    : session.restoring
      ? 'loading'
      : !session.ready
        ? 'unauthenticated'
        : policy
          ? 'ready'
          : 'loading';
  const nodes = policy?.nodes ?? [];
  return {
    phase,
    policy,
    nodes,
    error,
    allowsPath: (path) =>
      nodes.some(
        (node) => node.path === path || (node.path !== '/' && path.startsWith(`${node.path}/`)),
      ),
    refresh: async () => {
      if (session.ready) await refetch();
    },
  };
}

export function useNavigation() {
  const value = useContext(NavigationContext);
  if (!value) throw new Error('NavigationProvider is required');
  return value;
}
