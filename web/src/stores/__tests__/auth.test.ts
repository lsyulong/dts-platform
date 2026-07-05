import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useAuthStore } from '../auth';

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
    setItem: vi.fn((key: string, value: string) => { store[key] = value; }),
    removeItem: vi.fn((key: string) => { delete store[key]; }),
    clear: vi.fn(() => { store = {}; }),
  };
})();
Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });

describe('useAuthStore', () => {
  beforeEach(() => {
    useAuthStore.setState({
      token: null,
      user: null,
      loading: false,
    });
    localStorageMock.clear();
    vi.clearAllMocks();
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
      const user = { id: '1', username: 'admin', roles: [], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' };
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
        data: { token, user: { id: '1', username: 'admin', roles: [], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' } },
      });

      await useAuthStore.getState().login('admin', 'admin123', 'default');

      expect(localStorageMock.setItem).toHaveBeenCalledWith('token', token);
    });

    it('stores user in localStorage as JSON', async () => {
      const user = { id: '1', username: 'admin', roles: [], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' };
      mockClient.post.mockResolvedValueOnce({ data: { token: 'jwt', user } });

      await useAuthStore.getState().login('admin', 'admin123', 'default');

      expect(localStorageMock.setItem).toHaveBeenCalledWith('user', JSON.stringify(user));
    });

    it('throws error on failed login', async () => {
      mockClient.post.mockRejectedValueOnce(new Error('Invalid credentials'));

      await expect(
        useAuthStore.getState().login('wrong', 'wrong', 'default')
      ).rejects.toThrow('Invalid credentials');
    });
  });

  describe('logout', () => {
    it('clears token and user', () => {
      useAuthStore.setState({
        token: 'some-token',
        user: { id: '1', username: 'admin', roles: [], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' },
      });

      useAuthStore.getState().logout();

      const state = useAuthStore.getState();
      expect(state.token).toBeNull();
      expect(state.user).toBeNull();
    });

    it('removes token from localStorage', () => {
      useAuthStore.setState({ token: 'some-token' });

      useAuthStore.getState().logout();

      expect(localStorageMock.removeItem).toHaveBeenCalledWith('token');
    });

    it('removes user from localStorage', () => {
      useAuthStore.setState({ user: { id: '1', username: 'admin', roles: [], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' } });

      useAuthStore.getState().logout();

      expect(localStorageMock.removeItem).toHaveBeenCalledWith('user');
    });
  });

  describe('fetchMe', () => {
    it('updates user on successful fetch', async () => {
      const user = { id: '1', username: 'fetched-user', email: 'f@b.com', tenant_id: 't1', tenant_name: 'default', roles: [], permissions: [] };
      mockClient.get.mockResolvedValueOnce({ data: user });

      await useAuthStore.getState().fetchMe();

      expect(useAuthStore.getState().user).toEqual(user);
    });

    it('stores updated user in localStorage', async () => {
      const user = { id: '1', username: 'fetched-user', email: 'f@b.com', tenant_id: 't1', tenant_name: 'default', roles: [], permissions: [] };
      mockClient.get.mockResolvedValueOnce({ data: user });

      await useAuthStore.getState().fetchMe();

      expect(localStorageMock.setItem).toHaveBeenCalledWith('user', JSON.stringify(user));
    });

    it('calls logout on fetch failure', async () => {
      mockClient.get.mockRejectedValueOnce(new Error('Network error'));
      const logoutSpy = vi.spyOn(useAuthStore.getState(), 'logout');

      await useAuthStore.getState().fetchMe();

      expect(logoutSpy).toHaveBeenCalled();
    });
  });

  describe('hasPermission', () => {
    it('returns true when user has the permission', () => {
      useAuthStore.setState({
        user: { id: '1', username: 'admin', roles: [], permissions: ['task:read', 'task:write'], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' },
      });

      expect(useAuthStore.getState().hasPermission('task:read')).toBe(true);
    });

    it('returns false when user does not have the permission', () => {
      useAuthStore.setState({
        user: { id: '1', username: 'admin', roles: [], permissions: ['task:read'], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' },
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
        user: { id: '1', username: 'admin', roles: ['admin', 'operator'], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' },
      });

      expect(useAuthStore.getState().hasRole('admin')).toBe(true);
    });

    it('returns false when user does not have the role', () => {
      useAuthStore.setState({
        user: { id: '1', username: 'admin', roles: ['operator'], permissions: [], email: 'a@b.com', tenant_id: 't1', tenant_name: 'default' },
      });

      expect(useAuthStore.getState().hasRole('admin')).toBe(false);
    });

    it('returns false when user is null', () => {
      useAuthStore.setState({ user: null });

      expect(useAuthStore.getState().hasRole('admin')).toBe(false);
    });
  });
});
