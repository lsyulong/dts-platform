import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { server } from '../../test/mocks/server';
import { renderWithProviders } from '../../test/test-utils';
import { queryClient } from '../../lib/queryClient';
import { useAuthStore } from '../../stores/auth';
import { useAlerts, useCreateDatasource, useToggleAlert } from '../useApi';
import UsersTab from '../../pages/system/UsersTab';

const API = 'http://localhost:8080/api/v1';
const userA = {
  id: 'user-a',
  username: 'a',
  email: 'a@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'A',
  roles: [],
  permissions: [],
};
const userB = { ...userA, id: 'user-b', username: 'b', tenant_id: 'tenant-b', tenant_name: 'B' };
const alertA = {
  id: 'alert-a',
  name: 'A private alert',
  enabled: true,
  metric_type: 'error_count',
  operator: 'gt',
  threshold: 1,
  task_name: null,
  created_at: '',
};
const alertB = { ...alertA, id: 'alert-b', name: 'B private alert' };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => {
    resolve = finish;
  });
  return { promise, resolve };
}

function Alerts({ onMutation }: { onMutation?: (pending: Promise<unknown>) => void }) {
  const { data = [], isFetching } = useAlerts();
  const toggle = useToggleAlert();
  return (
    <>
      <p>{isFetching ? 'Refreshing alerts' : 'Alerts ready'}</p>
      {data.map((alert) => (
        <p key={alert.id}>
          {alert.name}: {alert.enabled ? 'enabled' : 'disabled'}
        </p>
      ))}
      <button onClick={() => onMutation?.(toggle.mutateAsync('alert-a').catch(() => undefined))}>
        Toggle A
      </button>
    </>
  );
}

function CreateDatasource({ onMutation }: { onMutation: (pending: Promise<unknown>) => void }) {
  const create = useCreateDatasource();
  return (
    <button
      onClick={() => onMutation(create.mutateAsync({ name: 'A source' }).catch(() => undefined))}
    >
      Create A source
    </button>
  );
}

function loginA() {
  localStorage.setItem('token', 'session-a');
  localStorage.setItem('user', JSON.stringify(userA));
  useAuthStore.setState({
    token: 'session-a',
    user: userA,
    initialized: true,
    restoring: false,
    restoreError: null,
  });
}

async function switchToB() {
  await useAuthStore.getState().logout();
  await useAuthStore.getState().login('b', 'password-b', 'tenant-b');
}

describe('Mutation session isolation', () => {
  beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
  afterAll(() => server.close());
  beforeEach(() => {
    queryClient.clear();
    localStorage.clear();
    loginA();
    server.use(
      http.post(`${API}/auth/login`, () => HttpResponse.json({ token: 'session-b', user: userB })),
      http.get(`${API}/alerts`, ({ request }) =>
        HttpResponse.json({
          data: request.headers.get('Authorization') === 'Bearer session-b' ? [alertB] : [alertA],
        }),
      ),
    );
  });
  afterEach(() => {
    server.resetHandlers();
    queryClient.clear();
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it.each([401, 500, 200])(
    'preserves B alerts when a request from the previous A session finishes with HTTP %s',
    async (status) => {
      const response = deferred<Response>();
      const started = deferred<void>();
      server.use(
        http.patch(`${API}/alerts/:id/toggle`, async () => {
          started.resolve();
          return response.promise;
        }),
      );
      let pending: Promise<unknown> | undefined;
      const interactions = userEvent.setup();
      const first = renderWithProviders(
        <Alerts
          onMutation={(value) => {
            pending = value;
          }}
        />,
        { queryClient },
      );
      expect(await screen.findByText('A private alert: enabled')).toBeInTheDocument();
      await interactions.click(screen.getByText('Toggle A'));
      await started.promise;
      expect(await screen.findByText('A private alert: disabled')).toBeInTheDocument();
      first.unmount();
      await switchToB();
      renderWithProviders(<Alerts />, { queryClient });
      expect(await screen.findByText('B private alert: enabled')).toBeInTheDocument();
      const refresh = deferred<Response>();
      server.use(http.get(`${API}/alerts`, () => refresh.promise));
      try {
        await act(async () => {
          response.resolve(HttpResponse.json({ message: 'Late A response' }, { status }));
          await pending;
        });
        expect(queryClient.getQueryData(['alerts'])).toEqual([alertB]);
        expect(queryClient.getQueryState(['alerts'])?.isInvalidated).toBe(false);
        expect(screen.queryByText(/A private alert/)).not.toBeInTheDocument();
        expect(screen.getByText('B private alert: enabled')).toBeInTheDocument();
      } finally {
        refresh.resolve(HttpResponse.json({ data: [alertB] }));
      }
    },
  );

  it('does not invalidate B datasources when an A creation finishes successfully after the account switch', async () => {
    const response = deferred<Response>();
    const started = deferred<void>();
    server.use(
      http.post(`${API}/datasources`, async () => {
        started.resolve();
        return response.promise;
      }),
    );
    let pending: Promise<unknown> | undefined;
    const interactions = userEvent.setup();
    const first = renderWithProviders(
      <CreateDatasource
        onMutation={(value) => {
          pending = value;
        }}
      />,
      { queryClient },
    );
    await interactions.click(screen.getByText('Create A source'));
    await started.promise;
    first.unmount();
    await switchToB();
    const bData = [{ id: 'b-source', name: 'B datasource' }];
    queryClient.setQueryData(['datasources'], bData);
    await act(async () => {
      response.resolve(HttpResponse.json({ id: 'a-source' }, { status: 201 }));
      await pending;
    });
    expect(queryClient.getQueryState(['datasources'])?.isInvalidated).toBe(false);
    expect(queryClient.getQueryData(['datasources'])).toEqual(bData);
  });

  it('does not invalidate B users when an old A management request completes', async () => {
    const response = deferred<Response>();
    const started = deferred<void>();
    const managedA = { ...userA, id: 'managed-a', status: 'active', created_at: '' };
    server.use(
      http.get(`${API}/system/users`, () => HttpResponse.json({ data: [managedA] })),
      http.delete(`${API}/system/users/managed-a`, async () => {
        started.resolve();
        return response.promise;
      }),
    );
    vi.stubGlobal('confirm', () => true);
    const interactions = userEvent.setup();
    const first = renderWithProviders(<UsersTab />, { queryClient });
    expect(await screen.findByText('a@example.invalid')).toBeInTheDocument();
    await interactions.click(screen.getByTitle('common.delete'));
    await started.promise;
    const mutation = queryClient.getMutationCache().getAll()[0];
    first.unmount();
    await switchToB();
    const bData = [{ ...userB, id: 'managed-b', status: 'active', created_at: '' }];
    queryClient.setQueryData(['system', 'users'], bData);
    response.resolve(new HttpResponse(null, { status: 204 }));
    await waitFor(() => expect(mutation.state.status).toBe('success'));
    expect(queryClient.getQueryState(['system', 'users'])?.isInvalidated).toBe(false);
    expect(queryClient.getQueryData(['system', 'users'])).toEqual(bData);
  });

  it('rolls back an optimistic failure while the initiating session is still current', async () => {
    const response = deferred<Response>();
    const started = deferred<void>();
    server.use(
      http.patch(`${API}/alerts/:id/toggle`, async () => {
        started.resolve();
        return response.promise;
      }),
    );
    let pending: Promise<unknown> | undefined;
    const interactions = userEvent.setup();
    renderWithProviders(
      <Alerts
        onMutation={(value) => {
          pending = value;
        }}
      />,
      { queryClient },
    );
    expect(await screen.findByText('A private alert: enabled')).toBeInTheDocument();
    await interactions.click(screen.getByText('Toggle A'));
    await started.promise;
    expect(await screen.findByText('A private alert: disabled')).toBeInTheDocument();
    const refresh = deferred<Response>();
    server.use(http.get(`${API}/alerts`, () => refresh.promise));
    try {
      await act(async () => {
        response.resolve(HttpResponse.json({ message: 'Request failed' }, { status: 500 }));
        await pending;
      });
      expect(queryClient.getQueryData(['alerts'])).toEqual([alertA]);
      expect(useAuthStore.getState().token).toBe('session-a');
    } finally {
      refresh.resolve(HttpResponse.json({ data: [alertA] }));
    }
  });
});
