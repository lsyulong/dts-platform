import { queryClient } from './queryClient';

const clearedListeners = new Set<() => void>();

export interface SessionIdentity {
  token: string | null;
  tenantId: string | null;
  userId: string | null;
}

export function captureSession(): SessionIdentity {
  let user: { tenant_id?: unknown; id?: unknown } | null = null;
  try {
    user = JSON.parse(localStorage.getItem('user') || 'null');
  } catch {
    // The opaque token still scopes callbacks if cached user data is unreadable.
  }
  return {
    token: localStorage.getItem('token'),
    tenantId: typeof user?.tenant_id === 'string' ? user.tenant_id : null,
    userId: typeof user?.id === 'string' ? user.id : null,
  };
}

export function isCurrentSession(session: SessionIdentity | undefined): boolean {
  if (!session) return false;
  const current = captureSession();
  return (
    session.token === current.token &&
    session.tenantId === current.tenantId &&
    session.userId === current.userId
  );
}

export function queryHistoryKey(user: { tenant_id: string; id: string } | null): string | null {
  return user
    ? `dts-query-history:${encodeURIComponent(user.tenant_id)}:${encodeURIComponent(user.id)}`
    : null;
}

export function isUnauthorized(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'response' in error &&
    typeof error.response === 'object' &&
    error.response !== null &&
    'status' in error.response &&
    error.response.status === 401
  );
}

export function clearSession(expectedToken?: string | null): boolean {
  const currentToken = localStorage.getItem('token');
  if (expectedToken !== undefined && currentToken !== null && currentToken !== expectedToken)
    return false;
  try {
    const user = JSON.parse(localStorage.getItem('user') || 'null');
    if (typeof user?.id === 'string' && typeof user?.tenant_id === 'string') {
      const key = queryHistoryKey(user);
      if (key) localStorage.removeItem(key);
    }
  } catch {
    // A damaged cached user must not prevent clearing an invalid session.
  }
  localStorage.removeItem('dts-query-history');
  localStorage.removeItem('token');
  localStorage.removeItem('user');
  void queryClient.cancelQueries();
  queryClient.clear();
  clearedListeners.forEach((listener) => listener());
  return true;
}

export function onSessionCleared(listener: () => void): () => void {
  clearedListeners.add(listener);
  return () => clearedListeners.delete(listener);
}
