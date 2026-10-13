import { useSyncExternalStore } from 'react';
import { useAuthStore } from '../../stores/auth';

export interface NavigationSession {
  key: string;
  token: string | null;
  tenantId: string | null;
  userId: string | null;
  ready: boolean;
  restoring: boolean;
  error: string | null;
  canManage: boolean;
  canReadDefinitions: boolean;
}

let previous = '';
let sequence = 0;
let snapshot: NavigationSession;

export function navigationSession(): NavigationSession {
  const state = useAuthStore.getState();
  const permissions = [...(state.user?.permissions ?? [])].sort();
  const identity = JSON.stringify([
    state.token,
    state.user?.tenant_id,
    state.user?.id,
    [...(state.user?.roles ?? [])].sort(),
    permissions,
    state.initialized,
    state.restoring,
    state.restoreError,
  ]);
  if (identity !== previous) {
    previous = identity;
    snapshot = {
      key: `navigation-session-${++sequence}`,
      token: state.token,
      tenantId: state.user?.tenant_id ?? null,
      userId: state.user?.id ?? null,
      ready:
        !!state.token &&
        !!state.user &&
        state.initialized &&
        !state.restoring &&
        !state.restoreError,
      restoring: !state.initialized || state.restoring,
      error: state.restoreError,
      canManage: permissions.includes('system:write') && permissions.includes('user:manage'),
      canReadDefinitions: permissions.includes('system:read'),
    };
  }
  return snapshot;
}

export function isNavigationSession(session: NavigationSession) {
  return session.ready && navigationSession().key === session.key;
}

const subscribe = (listener: () => void) => useAuthStore.subscribe(listener);

export function useNavigationSession() {
  return useSyncExternalStore(subscribe, navigationSession, navigationSession);
}
