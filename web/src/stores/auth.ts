import { create } from 'zustand';
import client from '../api/client';

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
  login: (username: string, password: string, tenantSlug: string) => Promise<void>;
  logout: () => void;
  fetchMe: () => Promise<void>;
  hasPermission: (perm: string) => boolean;
  hasRole: (role: string) => boolean;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  token: localStorage.getItem('token'),
  user: JSON.parse(localStorage.getItem('user') || 'null'),
  loading: false,

  login: async (username, password, tenantSlug) => {
    set({ loading: true });
    const { data } = await client.post('/auth/login', {
      username,
      password,
      tenant_slug: tenantSlug,
    });
    localStorage.setItem('token', data.token);
    localStorage.setItem('user', JSON.stringify(data.user));
    set({ token: data.token, user: data.user, loading: false });
  },

  logout: () => {
    localStorage.removeItem('token');
    localStorage.removeItem('user');
    set({ token: null, user: null });
  },

  fetchMe: async () => {
    try {
      const { data } = await client.get('/auth/me');
      localStorage.setItem('user', JSON.stringify(data));
      set({ user: data });
    } catch {
      get().logout();
    }
  },

  hasPermission: (perm) => {
    return get().user?.permissions?.includes(perm) ?? false;
  },

  hasRole: (role) => {
    return get().user?.roles?.includes(role) ?? false;
  },
}));
