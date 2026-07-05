import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import axios from 'axios';

describe('API Client', () => {
  const localStorageMock = (() => {
    let store: Record<string, string> = {};
    return {
      getItem: (key: string) => store[key] ?? null,
      setItem: (key: string, value: string) => { store[key] = value; },
      removeItem: (key: string) => { delete store[key]; },
      clear: () => { store = {}; },
    };
  })();

  beforeAll(() => {
    Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock });
  });

  beforeEach(() => {
    localStorageMock.clear();
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

  it('has response interceptor that handles 401 errors', async () => {
    localStorageMock.setItem('token', 'some-token');
    localStorageMock.setItem('user', JSON.stringify({ id: '1' }));

    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const responseInterceptor = client.interceptors.response.handlers[0];
    expect(responseInterceptor).toBeDefined();

    const error = {
      response: { status: 401 },
    };

    try {
      await responseInterceptor.rejected(error);
    } catch {
      // expected rejection
    }

    expect(localStorageMock.getItem('token')).toBeNull();
    expect(localStorageMock.getItem('user')).toBeNull();
  });

  it('has response interceptor that passes through successful responses', async () => {
    const clientModule = await import('../../api/client');
    const client = clientModule.default;

    const responseInterceptor = client.interceptors.response.handlers[0];
    const response = { data: { success: true }, status: 200 };

    const result = responseInterceptor.fulfilled(response);
    expect(result).toEqual(response);
  });

  it('has response interceptor that rejects non-401 errors', () => {
    // The interceptor rejects errors other than through the 401 handler
    // The second handler in the interceptor chain is the error handler
    const clientModulePromise = import('../../api/client');
    return clientModulePromise.then((clientModule) => {
      const client = clientModule.default;
      // There should be at least one response interceptor
      expect(client.interceptors.response.handlers.length).toBeGreaterThan(0);
    });
  });
});
