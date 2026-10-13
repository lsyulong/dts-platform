import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { clearSession } from '../../../lib/session';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import ServiceCredentialsPanel from '../ServiceCredentialsPanel';
import ServiceCallsPage from '../ServiceCallsPage';

const API = 'http://localhost:8080/api/v1';
const SECRET = 'dts_app_once_only_secret_for_test';
const user = {
  id: 'service-owner',
  username: 'owner',
  email: 'owner@example.invalid',
  tenant_id: 'tenant-services',
  tenant_name: 'Services',
  roles: ['member'],
  permissions: ['services:read', 'services:write'],
};
const application = {
  id: 'app-a',
  tenant_id: user.tenant_id,
  kind: 'service_application',
  code: 'app-a',
  name: '报表调用应用',
  description: null,
  parent_id: null,
  status: 'active',
  version: 4,
  references: [],
  properties: {
    owner_id: user.id,
    allowed_origins: [],
    credential_expires_at: '2030-01-01T00:00:00Z',
  },
  created_by: user.id,
  created_at: '2026-10-08T10:00:00Z',
  updated_at: '2026-10-08T10:00:00Z',
};
const credential = {
  id: 'credential-a',
  tenant_id: user.tenant_id,
  application_id: application.id,
  owner_id: user.id,
  version: 1,
  created_by: user.id,
  created_at: '2026-10-08T10:00:00Z',
  expires_at: '2030-01-01T00:00:00Z',
  revoked_at: null,
  revoked_by: null,
};
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
function login(permissions = user.permissions) {
  const current = { ...user, permissions };
  localStorage.setItem('token', 'services-session');
  localStorage.setItem('user', JSON.stringify(current));
  useAuthStore.setState({
    user: current,
    token: 'services-session',
    initialized: true,
    restoring: false,
    restoreError: null,
  });
}
function issued(secret: string | null, replayed = false) {
  return {
    application,
    credential,
    issued: true,
    secret,
    replayed,
    secret_not_retrievable: replayed,
  };
}
function cachedValues(cache: ReturnType<typeof createTestQueryClient>) {
  return JSON.stringify({
    queries: cache
      .getQueryCache()
      .getAll()
      .map((query) => query.state.data),
    mutations: cache
      .getMutationCache()
      .getAll()
      .map((mutation) => mutation.state),
  });
}
async function chooseApplication() {
  const actions = userEvent.setup();
  await screen.findByRole('option', { name: /报表调用应用/ });
  await actions.selectOptions(screen.getByRole('combobox', { name: '调用应用' }), application.id);
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '签发新的调用凭据' })).toBeEnabled(),
  );
  return actions;
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/resources/service_application`, () =>
      HttpResponse.json({ data: [application], total: 1 }),
    ),
    http.get(`${API}/product/resources/service_application/:id`, () =>
      HttpResponse.json(application),
    ),
    http.get(`${API}/product/resources/data_api`, () => HttpResponse.json({ data: [], total: 0 })),
    http.get(`${API}/product/service-access/applications/:id/credentials`, () =>
      HttpResponse.json({ data: [credential], total: 1 }),
    ),
  );
});
afterEach(() => {
  cleanup();
  server.resetHandlers();
  vi.restoreAllMocks();
  act(() => useAuthStore.setState({ user: null, token: null }));
  localStorage.clear();
  sessionStorage.clear();
});

describe('service credentials', () => {
  it('keeps the same UUID and original version across failure, close and retry', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${API}/product/service-access/applications/:id/credentials`,
        async ({ request }) => {
          bodies.push(await request.json());
          return bodies.length === 1
            ? HttpResponse.json({ message: 'response lost' }, { status: 503 })
            : HttpResponse.json(issued(SECRET));
        },
      ),
    );
    renderWithProviders(<ServiceCredentialsPanel />);
    const actions = await chooseApplication();
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await screen.findByText('response lost');
    await actions.click(screen.getByRole('button', { name: '取消' }));
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await screen.findByText(SECRET);
    expect(bodies).toHaveLength(2);
    expect(bodies[0]).toEqual({
      expected_version: 4,
      idempotency_key: expect.stringMatching(UUID),
      expires_at: null,
    });
    expect(bodies[1]).toEqual(bodies[0]);
  });

  it('keeps a stable UUID when a credential revocation is retried', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${API}/product/service-access/applications/:app/credentials/:credential/revoke`,
        async ({ request }) => {
          bodies.push(await request.json());
          return bodies.length === 1
            ? HttpResponse.json({ message: 'revocation response lost' }, { status: 503 })
            : HttpResponse.json({
                ...credential,
                revoked_at: '2026-10-09T10:00:00Z',
                revoked_by: user.id,
                version: 2,
              });
        },
      ),
    );
    renderWithProviders(<ServiceCredentialsPanel />);
    const actions = await chooseApplication();
    await actions.click(await screen.findByRole('button', { name: '撤销凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await screen.findByText('revocation response lost');
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(bodies).toHaveLength(2);
    expect(bodies[0]).toEqual({
      expected_version: 1,
      idempotency_key: expect.stringMatching(UUID),
    });
    expect(bodies[1]).toEqual(bodies[0]);
  });

  it('shows a first-issued secret only in the modal and discards it on close', async () => {
    server.use(
      http.post(`${API}/product/service-access/applications/:id/credentials`, () =>
        HttpResponse.json(issued(SECRET)),
      ),
    );
    const cache = createTestQueryClient();
    renderWithProviders(<ServiceCredentialsPanel />, { queryClient: cache });
    const actions = await chooseApplication();
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    expect(await screen.findByText(SECRET)).toBeInTheDocument();
    const modal = screen.getByRole('dialog');
    expect(within(modal).getByText(SECRET)).toBeInTheDocument();
    expect(cachedValues(cache)).not.toContain(SECRET);
    expect(JSON.stringify(localStorage)).not.toContain(SECRET);
    expect(JSON.stringify(sessionStorage)).not.toContain(SECRET);
    await actions.click(within(modal).getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(screen.queryByText(SECRET)).not.toBeInTheDocument());
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    expect(screen.queryByText(SECRET)).not.toBeInTheDocument();
    expect(cachedValues(cache)).not.toContain(SECRET);
  });

  it('shows a replay receipt without inventing or retrieving a secret', async () => {
    server.use(
      http.post(`${API}/product/service-access/applications/:id/credentials`, () =>
        HttpResponse.json(issued(null, true)),
      ),
    );
    const cache = createTestQueryClient();
    renderWithProviders(<ServiceCredentialsPanel />, { queryClient: cache });
    const actions = await chooseApplication();
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    expect(await screen.findByText(/本次为重试回执/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '复制凭据' })).not.toBeInTheDocument();
    expect(screen.queryByText(SECRET)).not.toBeInTheDocument();
    expect(cachedValues(cache)).not.toContain(SECRET);
  });

  it('clears the displayed secret when the current account loses access', async () => {
    server.use(
      http.post(`${API}/product/service-access/applications/:id/credentials`, () =>
        HttpResponse.json(issued(SECRET)),
      ),
    );
    renderWithProviders(<ServiceCredentialsPanel />);
    const actions = await chooseApplication();
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await screen.findByText(SECRET);
    act(() => login([]));
    expect(screen.queryByText(SECRET)).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByText('当前账号没有服务查看权限')).toBeInTheDocument();
  });

  it('clears the secret on actual logout and renders the signed-out state safely', async () => {
    server.use(
      http.post(`${API}/product/service-access/applications/:id/credentials`, () =>
        HttpResponse.json(issued(SECRET)),
      ),
    );
    renderWithProviders(<ServiceCredentialsPanel />);
    const actions = await chooseApplication();
    await actions.click(screen.getByRole('button', { name: '签发新的调用凭据' }));
    await actions.click(screen.getByRole('button', { name: '确认' }));
    await screen.findByText(SECRET);
    act(() => {
      clearSession('services-session');
    });
    expect(screen.queryByText(SECRET)).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByText('当前账号没有服务查看权限')).toBeInTheDocument();
  });
});

describe('actual service call records', () => {
  it('keeps denied and pending observations unknown while displaying real zero values', async () => {
    const base = {
      tenant_id: user.tenant_id,
      owner_id: user.id,
      application_id: application.id,
      credential_id: credential.id,
      api_id: 'api-a',
      target_hash: 'a'.repeat(64),
      started_at: '2026-10-09T10:00:00Z',
      finished_at: '2026-10-09T10:00:00Z',
    };
    server.use(
      http.get(`${API}/product/service-access/calls`, () =>
        HttpResponse.json({
          data: [
            {
              ...base,
              id: 'call-denied',
              outcome: 'denied',
              reason_code: 'rate_limited',
              duration_ms: null,
              row_count: null,
              api_id: null,
            },
            {
              ...base,
              id: 'call-pending',
              outcome: 'pending',
              reason_code: null,
              duration_ms: null,
              row_count: null,
              finished_at: null,
            },
            {
              ...base,
              id: 'call-empty',
              outcome: 'succeeded',
              reason_code: null,
              duration_ms: 0,
              row_count: 0,
            },
          ],
          total: 3,
        }),
      ),
    );
    renderWithProviders(<ServiceCallsPage />);
    const actions = userEvent.setup();
    for (const [id, duration, rows] of [
      ['call-denied', '—', '—'],
      ['call-pending', '—', '—'],
      ['call-empty', '0 ms', '0'],
    ]) {
      const row = (await screen.findByText(id)).closest('tr');
      if (!row) throw new Error('Call record must appear in a table row');
      const cells = within(row).getAllByRole('cell');
      expect(cells[3]).toHaveTextContent(duration);
      expect(cells[4]).toHaveTextContent(rows);
    }
    expect(screen.getByText('超过每分钟限额')).toBeInTheDocument();
    expect(screen.getByText('目标未获准')).toBeInTheDocument();
    const pending = screen.getByText('call-pending').closest('tr');
    if (!pending) throw new Error('Pending record missing');
    await actions.click(within(pending).getByRole('button', { name: '详情' }));
    expect(screen.getByText(/连接中断或进程退出时不能据此判定成功/)).toBeInTheDocument();
  });
});
