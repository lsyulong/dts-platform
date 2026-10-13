import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { useAuthStore } from '../../stores/auth';
import { queryClient } from '../../lib/queryClient';

describe('API Client', () => {
  const localStorageMock = (() => {
    let store: Record<string, string> = {};
    return {
      getItem: (key: string) => store[key] ?? null,
      setItem: (key: string, value: string) => {
        store[key] = value;
      },
      removeItem: (key: string) => {
        delete store[key];
      },
      clear: () => {
        store = {};
      },
    };
  })();

  beforeAll(() => {
    Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });
  });

  beforeEach(() => {
    localStorageMock.clear();
    useAuthStore.setState({ token: null, user: null });
    queryClient.clear();
  });

  it('creates an axios instance with baseURL', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;
    expect(client.defaults.baseURL).toBeDefined();
    expect(client.defaults.baseURL).toMatch(/\/api\/v1$/);
  });

  it('sets Content-Type header to application/json', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;
    expect(client.defaults.headers['Content-Type'] as string).toBe('application/json');
  });

  it('sets timeout to 30000', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;
    expect(client.defaults.timeout).toBe(30000);
  });

  it('has request interceptor that attaches token from localStorage', async () => {
    localStorageMock.setItem('token', 'test-bearer-token');
    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const interceptor = client.interceptors.request.handlers[0];
    expect(interceptor).toBeDefined();

    const config = { headers: {} as Record<string, string> };
    const result = interceptor.fulfilled(config);

    expect(result.headers.Authorization).toBe('Bearer test-bearer-token');
  });

  it('has request interceptor that does not attach header when no token', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const interceptor = client.interceptors.request.handlers[0];
    const config = { headers: {} as Record<string, string> };
    const result = interceptor.fulfilled(config);

    expect(result.headers.Authorization).toBeUndefined();
  });

  it('preserves an explicit token captured for server logout', async () => {
    localStorageMock.setItem('token', 'new-session');
    const { default: client } = await import('../../api/client');
    const result = client.interceptors.request.handlers[0].fulfilled({
      headers: { Authorization: 'Bearer logout-session' },
    });
    expect(result.headers.Authorization).toBe('Bearer logout-session');
  });

  it('has response interceptor that handles 401 errors', async () => {
    localStorageMock.setItem('token', 'some-token');
    localStorageMock.setItem('user', JSON.stringify({ id: '1' }));
    useAuthStore.setState({ token: 'some-token' });
    queryClient.setQueryData(['private-data'], 'sensitive');

    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const responseInterceptor = client.interceptors.response.handlers[0];
    expect(responseInterceptor).toBeDefined();

    const error = {
      response: { status: 401 },
      config: { headers: { Authorization: 'Bearer some-token' } },
    };

    try {
      await responseInterceptor.rejected(error);
    } catch {
      // expected rejection
    }

    expect(localStorageMock.getItem('token')).toBeNull();
    expect(localStorageMock.getItem('user')).toBeNull();
    expect(useAuthStore.getState().token).toBeNull();
    expect(queryClient.getQueryData(['private-data'])).toBeUndefined();
  });

  it('does not expire a newer session when an old request returns 401', async () => {
    localStorageMock.setItem('token', 'new-session');
    useAuthStore.setState({ token: 'new-session' });
    const { default: client } = await import('../../api/client');
    const error = {
      response: { status: 401 },
      config: { headers: { Authorization: 'Bearer old-session' } },
    };
    await expect(client.interceptors.response.handlers[0].rejected(error)).rejects.toEqual(error);
    expect(useAuthStore.getState().token).toBe('new-session');
    expect(localStorageMock.getItem('token')).toBe('new-session');
  });

  it('leaves failed login requests as inline errors without expiring an existing session', async () => {
    localStorageMock.setItem('token', 'existing-session');
    const { default: client } = await import('../../api/client');
    const error = {
      response: { status: 401 },
      config: { url: '/auth/login', headers: { Authorization: 'Bearer existing-session' } },
    };
    await expect(client.interceptors.response.handlers[0].rejected(error)).rejects.toEqual(error);
    expect(localStorageMock.getItem('token')).toBe('existing-session');
  });

  it('has response interceptor that passes through successful responses', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const responseInterceptor = client.interceptors.response.handlers[0];
    const response = { data: { success: true }, status: 200 };

    const result = responseInterceptor.fulfilled(response);
    expect(result).toEqual(response);
  });

  it('retains the session and private data when the server returns an unrelated error', async () => {
    localStorageMock.setItem('token', 'valid-session');
    useAuthStore.setState({ token: 'valid-session' });
    queryClient.setQueryData(['private-data'], 'sensitive');
    const { default: client } = await import('../../api/client');
    const error = {
      response: { status: 500 },
      config: { headers: { Authorization: 'Bearer valid-session' } },
    };
    await expect(client.interceptors.response.handlers[0].rejected(error)).rejects.toEqual(error);
    expect(useAuthStore.getState().token).toBe('valid-session');
    expect(queryClient.getQueryData(['private-data'])).toBe('sensitive');
  });
});
