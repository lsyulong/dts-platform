import { create } from 'zustand';
import client from '../api/client';
import { clearSession, isUnauthorized, onSessionCleared } from '../lib/session';
import { queryClient } from '../lib/queryClient';
import { apiErrorMessage } from '../lib/apiError';

interface UserInfo {
  id: string;
  username: string;
  email: string;
  display_name?: string;
  tenant_id: string;
  tenant_name: string;
  roles: string[];
  permissions: string[];
}

interface AuthState {
  token: string | null;
  user: UserInfo | null;
  loading: boolean;
  initialized: boolean;
  restoring: boolean;
  restoreError: string | null;
  login: (username: string, password: string, tenantSlug: string) => Promise<void>;
  logout: () => Promise<void>;
  fetchMe: () => Promise<void>;
  hasPermission: (perm: string) => boolean;
  hasRole: (role: string) => boolean;
}

function cachedUser(): UserInfo | null {
  try {
    return JSON.parse(localStorage.getItem('user') || 'null');
  } catch {
    return null;
  }
}

export const useAuthStore = create<AuthState>((set, get) => ({
  token: localStorage.getItem('token'),
  user: cachedUser(),
  loading: false,
  initialized: !localStorage.getItem('token'),
  restoring: false,
  restoreError: null,

  login: async (username, password, tenantSlug) => {
    set({ loading: true });
    try {
      const { data } = await client.post('/auth/login', {
        username,
        password,
        tenant_slug: tenantSlug,
      });
      void queryClient.cancelQueries();
      queryClient.clear();
      localStorage.removeItem('dts-query-history');
      localStorage.setItem('token', data.token);
      localStorage.setItem('user', JSON.stringify(data.user));
      set({
        token: data.token,
        user: data.user,
        initialized: true,
        restoring: false,
        restoreError: null,
      });
    } finally {
      set({ loading: false });
    }
  },

  logout: async () => {
    const token = get().token || localStorage.getItem('token');
    if (token) {
      try {
        await client.post('/auth/logout', undefined, {
          headers: { Authorization: `Bearer ${token}` },
        });
      } catch (error) {
        if (!isUnauthorized(error)) throw error;
      }
    }
    clearSession(token);
  },

  fetchMe: async () => {
    if (get().restoring) return;
    const token = get().token;
    if (!token) return;
    set({ restoring: true, restoreError: null });
    try {
      const { data } = await client.get('/auth/me');
      if (get().token !== token) return;
      localStorage.setItem('user', JSON.stringify(data));
      set({ user: data, initialized: true });
    } catch (error) {
      if (isUnauthorized(error)) clearSession(token);
      else if (get().token === token)
        set({ restoreError: apiErrorMessage(error, 'Session verification failed') });
    } finally {
      if (get().token === token) set({ restoring: false });
    }
  },

  hasPermission: (perm) => {
    return get().user?.permissions?.includes(perm) ?? false;
  },

  hasRole: (role) => {
    return get().user?.roles?.includes(role) ?? false;
  },
}));

onSessionCleared(() =>
  useAuthStore.setState({
    token: null,
    user: null,
    loading: false,
    initialized: true,
    restoring: false,
    restoreError: null,
  }),
);

window.addEventListener('storage', (event) => {
  if (event.key !== null && event.key !== 'token') return;
  void queryClient.cancelQueries();
  queryClient.clear();
  const token = localStorage.getItem('token');
  useAuthStore.setState({
    token,
    user: null,
    initialized: !token,
    restoring: false,
    restoreError: null,
  });
});
