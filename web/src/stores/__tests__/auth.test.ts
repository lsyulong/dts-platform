import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useAuthStore } from '../auth';
import { queryClient } from '../../lib/queryClient';
import { queryHistoryKey } from '../../lib/session';

// Mock the API client
vi.mock('../../api/client', () => ({
  default: {
    post: vi.fn(),
    get: vi.fn(),
  },
}));

import client from '../../api/client';

const mockClient = client as unknown as {
  post: ReturnType<typeof vi.fn>;
  get: ReturnType<typeof vi.fn>;
};

// Mock localStorage
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: vi.fn((key: string) => store[key] ?? null),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = value;
    }),
    removeItem: vi.fn((key: string) => {
      delete store[key];
    }),
    clear: vi.fn(() => {
      store = {};
    }),
  };
})();
Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });

describe('useAuthStore', () => {
  beforeEach(() => {
    useAuthStore.setState({
      token: null,
      user: null,
      loading: false,
      initialized: false,
      restoring: false,
      restoreError: null,
    });
    localStorageMock.clear();
    vi.clearAllMocks();
    queryClient.clear();
  });

  describe('initial state', () => {
    it('has null token by default', () => {
      const state = useAuthStore.getState();
      expect(state.token).toBeNull();
    });

    it('has null user by default', () => {
      const state = useAuthStore.getState();
      expect(state.user).toBeNull();
    });

    it('has loading false by default', () => {
      const state = useAuthStore.getState();
      expect(state.loading).toBe(false);
    });
  });

  describe('login', () => {
    it('sets token and user on successful login', async () => {
      const user = {
        id: '1',
        username: 'admin',
        roles: [],
        permissions: [],
        email: 'a@b.com',
        tenant_id: 't1',
        tenant_name: 'default',
      };
      const token = 'test-jwt';
      mockClient.post.mockResolvedValueOnce({ data: { token, user } });

      await useAuthStore.getState().login('admin', 'admin123', 'default');

      const state = useAuthStore.getState();
      expect(state.token).toBe(token);
      expect(state.user).toEqual(user);
      expect(state.loading).toBe(false);
    });

    it('sets loading to true during login', () => {
      mockClient.post.mockReturnValueOnce(new Promise(() => {}));

      useAuthStore.getState().login('admin', 'admin123', 'default');

      expect(useAuthStore.getState().loading).toBe(true);
    });

    it('stores token in localStorage', async () => {
      const token = 'test-jwt';
      mockClient.post.mockResolvedValueOnce({
        data: {
          token,
          user: {
            id: '1',
            username: 'admin',
            roles: [],
            permissions: [],
            email: 'a@b.com',
            tenant_id: 't1',
            tenant_name: 'default',
          },
        },
      });

      await useAuthStore.getState().login('admin', 'admin123', 'default');

      expect(localStorageMock.setItem).toHaveBeenCalledWith('token', token);
    });

    it('stores user in localStorage as JSON', async () => {
      const user = {
        id: '1',
        username: 'admin',
        roles: [],
        permissions: [],
        email: 'a@b.com',
        tenant_id: 't1',
        tenant_name: 'default',
      };
      mockClient.post.mockResolvedValueOnce({ data: { token: 'jwt', user } });

      await useAuthStore.getState().login('admin', 'admin123', 'default');

      expect(localStorageMock.setItem).toHaveBeenCalledWith('user', JSON.stringify(user));
    });

    it('throws error on failed login', async () => {
      mockClient.post.mockRejectedValueOnce(new Error('Invalid credentials'));

      await expect(useAuthStore.getState().login('wrong', 'wrong', 'default')).rejects.toThrow(
        'Invalid credentials',
      );
      expect(useAuthStore.getState().loading).toBe(false);
    });
  });

  describe('logout', () => {
    it('clears token and user', async () => {
      useAuthStore.setState({
        token: 'some-token',
        user: {
          id: '1',
          username: 'admin',
          roles: [],
          permissions: [],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      await useAuthStore.getState().logout();

      const state = useAuthStore.getState();
      expect(state.token).toBeNull();
      expect(state.user).toBeNull();
    });

    it('removes token from localStorage', async () => {
      useAuthStore.setState({ token: 'some-token' });

      await useAuthStore.getState().logout();

      expect(localStorageMock.removeItem).toHaveBeenCalledWith('token');
    });

    it('removes user from localStorage', async () => {
      useAuthStore.setState({
        user: {
          id: '1',
          username: 'admin',
          roles: [],
          permissions: [],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      await useAuthStore.getState().logout();

      expect(localStorageMock.removeItem).toHaveBeenCalledWith('user');
    });
  });

  describe('fetchMe', () => {
    beforeEach(() => {
      localStorageMock.setItem('token', 'opaque-session');
      useAuthStore.setState({ token: 'opaque-session' });
    });
    it('updates user on successful fetch', async () => {
      const user = {
        id: '1',
        username: 'fetched-user',
        email: 'f@b.com',
        tenant_id: 't1',
        tenant_name: 'default',
        roles: [],
        permissions: [],
      };
      mockClient.get.mockResolvedValueOnce({ data: user });

      await useAuthStore.getState().fetchMe();

      expect(useAuthStore.getState().user).toEqual(user);
    });

    it('stores updated user in localStorage', async () => {
      const user = {
        id: '1',
        username: 'fetched-user',
        email: 'f@b.com',
        tenant_id: 't1',
        tenant_name: 'default',
        roles: [],
        permissions: [],
      };
      mockClient.get.mockResolvedValueOnce({ data: user });

      await useAuthStore.getState().fetchMe();

      expect(localStorageMock.setItem).toHaveBeenCalledWith('user', JSON.stringify(user));
    });

    it('keeps a session on network failure without attempting server logout', async () => {
      mockClient.get.mockRejectedValueOnce(new Error('Network error'));
      await useAuthStore.getState().fetchMe();
      expect(useAuthStore.getState().token).toBe('opaque-session');
      expect(useAuthStore.getState().restoreError).toBe('Network error');
      expect(mockClient.post).not.toHaveBeenCalled();
    });

    it('clears an expired session and private query data without calling logout', async () => {
      queryClient.setQueryData(['private'], 'sensitive');
      mockClient.get.mockRejectedValueOnce({ response: { status: 401 } });
      await useAuthStore.getState().fetchMe();
      expect(useAuthStore.getState().token).toBeNull();
      expect(localStorageMock.getItem('token')).toBeNull();
      expect(queryClient.getQueryData(['private'])).toBeUndefined();
      expect(mockClient.post).not.toHaveBeenCalled();
    });
  });

  it('waits for server revocation before clearing an opaque token', async () => {
    const token = 'fd143b45-2e8b-4e87-a084-0d6de4ad7438';
    const user = { id: 'reader', tenant_id: 'workspace' };
    localStorageMock.setItem('token', token);
    localStorageMock.setItem('user', JSON.stringify(user));
    localStorageMock.setItem(queryHistoryKey(user)!, 'private SQL history');
    localStorageMock.setItem('dts-query-history', 'old unscoped SQL');
    useAuthStore.setState({ token });
    queryClient.setQueryData(['private'], 'sensitive');
    let finish: (() => void) | undefined;
    mockClient.post.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    const logout = useAuthStore.getState().logout();
    expect(mockClient.post).toHaveBeenCalledWith('/auth/logout', undefined, {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(useAuthStore.getState().token).toBe(token);
    finish?.();
    await logout;
    expect(useAuthStore.getState().token).toBeNull();
    expect(localStorageMock.getItem('dts-query-history')).toBeNull();
    expect(localStorageMock.getItem(queryHistoryKey(user)!)).toBeNull();
    expect(queryClient.getQueryData(['private'])).toBeUndefined();
  });

  it('does not replace a newer session with an earlier restoration response', async () => {
    localStorageMock.setItem('token', 'old-session');
    useAuthStore.setState({ token: 'old-session' });
    let finish: ((value: { data: { id: string } }) => void) | undefined;
    mockClient.get.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const pending = useAuthStore.getState().fetchMe();
    localStorageMock.setItem('token', 'new-session');
    useAuthStore.setState({ token: 'new-session', user: null, restoring: false });
    finish?.({ data: { id: 'old-user' } });
    await pending;
    expect(useAuthStore.getState().token).toBe('new-session');
    expect(useAuthStore.getState().user).toBeNull();
    expect(localStorageMock.getItem('user')).toBeNull();
  });

  it('retains the session when server revocation fails but accepts an already expired token', async () => {
    localStorageMock.setItem('token', 'opaque');
    useAuthStore.setState({ token: 'opaque' });
    mockClient.post.mockRejectedValueOnce(new Error('Server unavailable'));
    await expect(useAuthStore.getState().logout()).rejects.toThrow('Server unavailable');
    expect(useAuthStore.getState().token).toBe('opaque');
    mockClient.post.mockRejectedValueOnce({ response: { status: 401 } });
    await useAuthStore.getState().logout();
    expect(useAuthStore.getState().token).toBeNull();
  });

  it('clears private caches and requires verification when another tab changes the token', () => {
    useAuthStore.setState({ token: 'previous-session', initialized: true });
    queryClient.setQueryData(['private'], 'previous tenant data');
    localStorageMock.setItem('token', 'other-tab-session');
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'token', newValue: 'other-tab-session' }),
    );
    expect(useAuthStore.getState().token).toBe('other-tab-session');
    expect(useAuthStore.getState().user).toBeNull();
    expect(useAuthStore.getState().initialized).toBe(false);
    expect(queryClient.getQueryData(['private'])).toBeUndefined();
    localStorageMock.clear();
    window.dispatchEvent(new StorageEvent('storage', { key: null }));
    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().initialized).toBe(true);
  });

  describe('hasPermission', () => {
    it('returns true when user has the permission', () => {
      useAuthStore.setState({
        user: {
          id: '1',
          username: 'admin',
          roles: [],
          permissions: ['task:read', 'task:write'],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      expect(useAuthStore.getState().hasPermission('task:read')).toBe(true);
    });

    it('returns false when user does not have the permission', () => {
      useAuthStore.setState({
        user: {
          id: '1',
          username: 'admin',
          roles: [],
          permissions: ['task:read'],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      expect(useAuthStore.getState().hasPermission('task:delete')).toBe(false);
    });

    it('returns false when user is null', () => {
      useAuthStore.setState({ user: null });

      expect(useAuthStore.getState().hasPermission('task:read')).toBe(false);
    });
  });

  describe('hasRole', () => {
    it('returns true when user has the role', () => {
      useAuthStore.setState({
        user: {
          id: '1',
          username: 'admin',
          roles: ['admin', 'operator'],
          permissions: [],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      expect(useAuthStore.getState().hasRole('admin')).toBe(true);
    });

    it('returns false when user does not have the role', () => {
      useAuthStore.setState({
        user: {
          id: '1',
          username: 'admin',
          roles: ['operator'],
          permissions: [],
          email: 'a@b.com',
          tenant_id: 't1',
          tenant_name: 'default',
        },
      });

      expect(useAuthStore.getState().hasRole('admin')).toBe(false);
    });

    it('returns false when user is null', () => {
      useAuthStore.setState({ user: null });

      expect(useAuthStore.getState().hasRole('admin')).toBe(false);
    });
  });
});
