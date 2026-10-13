import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { server } from '../../../test/mocks/server';
import { useAuthStore } from '../../../stores/auth';
import { fetchSelectionOptions } from '../option-sources';

const API = 'http://localhost:8080/api/v1';
const userA = {
  id: 'editor-a',
  username: 'editor-a',
  email: 'editor-a@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'Tenant A',
  roles: [],
  permissions: [],
};
const requestOptions = {
  source: 'users' as const,
  tenantId: 'tenant-a',
  userId: 'editor-a',
  token: 'synthetic-session-a',
  q: '',
  offset: 0,
  language: 'en',
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => {
    resolve = finish;
  });
  return { promise, resolve };
}

describe('Tenant-scoped selection options', () => {
  let previousAuth = useAuthStore.getState();

  beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
  afterAll(() => server.close());
  beforeEach(() => {
    previousAuth = useAuthStore.getState();
    useAuthStore.setState({ token: 'synthetic-session-a', user: userA });
  });
  afterEach(() => {
    server.resetHandlers();
    useAuthStore.setState({ token: previousAuth.token, user: previousAuth.user });
  });

  it('uses GET with tenant and search parameters and retains only safe user option fields', async () => {
    const requests: Request[] = [];
    server.use(
      http.get(`${API}/system/users`, ({ request }) => {
        requests.push(request);
        return HttpResponse.json({
          data: [
            {
              id: 'member-alice',
              tenant_id: 'tenant-a',
              username: 'alice',
              display_name: 'Alice Example',
              password_hash: 'synthetic-password-hash',
              token: 'synthetic-private-token',
              properties: { api_key: 'synthetic-private-metadata' },
            },
            {
              id: 'member-bob',
              tenant_id: 'tenant-a',
              username: 'bob',
              display_name: 'Bob Example',
            },
          ],
          total: 900,
        });
      }),
    );

    const options = await fetchSelectionOptions({
      ...requestOptions,
      q: 'alice',
      signal: new AbortController().signal,
    });

    expect(requests).toHaveLength(1);
    expect(requests[0].method).toBe('GET');
    expect(new URL(requests[0].url).pathname).toBe('/api/v1/system/users');
    expect(Object.fromEntries(new URL(requests[0].url).searchParams)).toEqual({
      tenant_id: 'tenant-a',
      search: 'alice',
    });
    expect(requests[0].headers.get('Authorization')).toBe('Bearer synthetic-session-a');
    expect(options).toEqual({
      data: [{ id: 'member-alice', label: 'Alice Example', detail: 'alice' }],
      total: 1,
    });
  });

  it('rejects the whole page when a user belongs to a different tenant', async () => {
    server.use(
      http.get(`${API}/system/users`, () =>
        HttpResponse.json({
          data: [
            { id: 'member-a', tenant_id: 'tenant-a', username: 'alice' },
            { id: 'member-b', tenant_id: 'tenant-b', username: 'bob' },
          ],
        }),
      ),
    );

    await expect(
      fetchSelectionOptions({ ...requestOptions, signal: new AbortController().signal }),
    ).rejects.toThrow('The available objects have an invalid tenant scope. Reload the list.');
  });

  it('rejects a delayed response after the requesting login session changes', async () => {
    const started = deferred<void>();
    const response = deferred<Response>();
    server.use(
      http.get(`${API}/system/users`, () => {
        started.resolve();
        return response.promise;
      }),
    );

    const pending = fetchSelectionOptions({
      ...requestOptions,
      signal: new AbortController().signal,
    });
    const rejected = expect(pending).rejects.toThrow('The login session changed. Reload the list.');
    await started.promise;
    useAuthStore.setState({
      token: 'synthetic-session-b',
      user: { ...userA, id: 'editor-b', tenant_id: 'tenant-b', tenant_name: 'Tenant B' },
    });
    response.resolve(
      HttpResponse.json({
        data: [{ id: 'member-a', tenant_id: 'tenant-a', username: 'alice' }],
      }),
    );

    await rejected;
  });

  it('uses a bounded module-authorized user directory without changing its server-side total', async () => {
    let called = '';
    server.use(
      http.get(`${API}/product/reference/users`, ({ request }) => {
        called = request.url;
        return HttpResponse.json({
          data: [
            { id: 'member-51', tenant_id: 'tenant-a', username: 'alice', display_name: 'Alice' },
          ],
          total: 120,
        });
      }),
    );
    const result = await fetchSelectionOptions({
      ...requestOptions,
      resourceKind: 'user_group',
      q: 'alice',
      offset: 50,
      signal: new AbortController().signal,
    });
    expect(Object.fromEntries(new URL(called).searchParams)).toEqual({
      resource_kind: 'user_group',
      q: 'alice',
      limit: '50',
      offset: '50',
    });
    expect(result).toEqual({
      data: [{ id: 'member-51', label: 'Alice', detail: 'alice' }],
      total: 120,
    });
  });
});
