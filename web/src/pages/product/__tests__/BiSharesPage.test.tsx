import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { QueryClient } from '@tanstack/react-query';
import { useLocation } from 'react-router-dom';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProductModule } from '../../../product/catalog-types';
import { useAuthStore } from '../../../stores/auth';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import BiReportPage from '../BiReportPage';
import BiSharesPage, { type BiShare } from '../BiSharesPage';
import type { ProductJob } from '../jobs-api';

vi.mock('../../../product/ProductModulePage', () => ({
  default: () => <p>Report definition workspace</p>,
}));
vi.mock('../JobRunsPage', () => ({ default: () => <p>Actual report runs workspace</p> }));

const API = 'http://localhost:8080/api/v1';
const PRIVATE_ROW = 'Current-reader authorized private amount';
const user = {
  id: 'receiver-a',
  username: 'receiver',
  email: 'receiver@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'Tenant A',
  roles: ['bi_reader'],
  permissions: ['bi:read'],
};
const share: BiShare = {
  id: 'real-share',
  tenant_id: 'tenant-a',
  report_id: 'actual-report',
  report_version: 4,
  job_id: 'actual-run',
  node_id: 'report',
  owner_id: 'creator-a',
  recipient_id: user.id,
  status: 'active',
  version: 1,
  expires_at: '2030-10-09T10:00:00Z',
  is_expired: false,
  created_at: '2026-10-09T10:00:00Z',
  revoked_at: null,
};
const job: ProductJob = {
  id: share.job_id,
  source_kind: 'bi_report',
  source_id: share.report_id,
  source_version: share.report_version,
  status: 'succeeded',
  cancel_requested: false,
  can_retry: true,
  attempt: 1,
  created_at: share.created_at,
  started_at: share.created_at,
  finished_at: share.created_at,
  error: null,
  result_access: 'authorized',
  result_access_reason: null,
  nodes: [
    {
      id: share.node_id,
      name: 'Actual report query',
      status: 'succeeded',
      depends_on: [],
      attempt: 1,
      result_access: 'authorized',
      result_access_reason: null,
      error: null,
      result: {
        columns: [{ name: 'amount', data_type: 'VARCHAR', is_numeric: false }],
        rows: [{ amount: PRIVATE_ROW }],
        row_count: 1,
        complete: true,
      },
    },
  ],
};
const module: ProductModule = {
  id: 'bi-reports',
  groupId: 'bi',
  title: { zh: 'BI 报表', en: 'BI reports' },
  description: { zh: '', en: '' },
  path: '/bi/reports',
  reference: [],
  implementation: 'new',
  resourceKind: 'bi_report',
};

let cache: QueryClient;
function login(account = user, token = 'receiver-session-a') {
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(account));
  useAuthStore.setState({
    user: account,
    token,
    initialized: true,
    restoring: false,
    restoreError: null,
  });
}
function listHandler(value: BiShare = share) {
  return http.get(`${API}/product/bi-shares`, ({ request }) => {
    expect(request.headers.get('Authorization')).toBe('Bearer receiver-session-a');
    return HttpResponse.json({ data: [value], total: 1 });
  });
}
function openHandler(value = { share, job }) {
  return http.get(`${API}/product/bi-shares/:id/open`, ({ request, params }) => {
    expect(params.id).toBe(share.id);
    expect(request.headers.get('Authorization')).toBe('Bearer receiver-session-a');
    return HttpResponse.json(value);
  });
}
function renderShares() {
  return renderWithProviders(<BiSharesPage />, {
    route: `/bi/reports?share=${share.id}`,
    queryClient: cache,
  });
}
function assertPrivateResultHidden() {
  expect(screen.queryByText(PRIVATE_ROW)).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '导出 JSON' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '导出 CSV' })).not.toBeInTheDocument();
}
function LocationProbe() {
  const location = useLocation();
  return <p data-testid="current-location">{location.pathname + location.search}</p>;
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  localStorage.clear();
  cache = createTestQueryClient();
  login();
  server.use(listHandler(), openHandler());
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('Authenticated report sharing and current-reader output', () => {
  it('opens the sharing tab from a real share query and clears the share when changing workspaces', async () => {
    renderWithProviders(
      <>
        <BiReportPage module={module} />
        <LocationProbe />
      </>,
      { route: `/bi/reports?share=${share.id}`, queryClient: cache },
    );
    expect(screen.getByRole('tab', { name: '报表分享' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.queryByText('Report definition workspace')).not.toBeInTheDocument();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('tab', { name: '报表定义' }));
    expect(screen.getByRole('tab', { name: '报表定义' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('Report definition workspace')).toBeInTheDocument();
    expect(screen.getByTestId('current-location')).toHaveTextContent(/^\/bi\/reports$/);
    assertPrivateResultHidden();
  });

  it('hides cached private rows immediately when a current-reader authorization refresh is denied', async () => {
    renderShares();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    server.use(
      http.get(`${API}/product/bi-shares/:id/open`, () =>
        HttpResponse.json({ message: 'Current data grant revoked' }, { status: 403 }),
      ),
    );
    await act(async () => {
      await cache.refetchQueries({ queryKey: ['bi-share-open'] });
    });
    expect(await screen.findByText('Current data grant revoked')).toBeInTheDocument();
    assertPrivateResultHidden();
    // React Query retains successful data after refetch errors. Its presence
    // must not make the old authorized rows visible again.
    const retained = cache.getQueriesData<{ share: BiShare; job: ProductJob }>({
      queryKey: ['bi-share-open'],
    })[0]?.[1];
    expect(retained?.job.nodes[0].result?.rows).toEqual([{ amount: PRIVATE_ROW }]);
    expect(useAuthStore.getState().token).toBe('receiver-session-a');
  });

  it('immediately removes a previous account output while a new account authorization is pending', async () => {
    renderShares();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/bi-shares`, () => HttpResponse.json({ data: [], total: 0 })),
      http.get(`${API}/product/bi-shares/:id/open`, async ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer receiver-session-b');
        await pending;
        return HttpResponse.json(
          { message: 'This share belongs to another user' },
          { status: 403 },
        );
      }),
    );
    await act(async () => {
      login({ ...user, id: 'receiver-b', username: 'other' }, 'receiver-session-b');
    });
    assertPrivateResultHidden();
    await act(async () => release());
    expect(await screen.findByText('This share belongs to another user')).toBeInTheDocument();
    assertPrivateResultHidden();
    expect(useAuthStore.getState().token).toBe('receiver-session-b');
  });

  it('does not reuse previous output after the same session data permission changes', async () => {
    login({ ...user, permissions: ['bi:read', 'data_security:all_data'] });
    renderShares();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/bi-shares/:id/open`, async () => {
        await pending;
        return HttpResponse.json(
          { message: 'Current data authorization required' },
          { status: 403 },
        );
      }),
    );
    try {
      await act(async () => login(user));
      assertPrivateResultHidden();
    } finally {
      await act(async () => release());
    }
    expect(await screen.findByText('Current data authorization required')).toBeInTheDocument();
    assertPrivateResultHidden();
  });

  it('hides an opened output when the current share-list authorization is rejected', async () => {
    renderShares();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    server.use(
      http.get(`${API}/product/bi-shares`, () =>
        HttpResponse.json({ message: 'Current BI reader permission revoked' }, { status: 403 }),
      ),
    );
    await act(async () => {
      await cache.refetchQueries({ queryKey: ['bi-shares'] });
    });
    expect(await screen.findByText('Current BI reader permission revoked')).toBeInTheDocument();
    assertPrivateResultHidden();
  });

  it('removes private output when BI read permission is withdrawn from the current session', async () => {
    renderShares();
    expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
    await act(async () => login({ ...user, permissions: [] }));
    expect(screen.getByText('当前账号没有 BI 查看权限')).toBeInTheDocument();
    assertPrivateResultHidden();
  });

  it.each([
    { state: 'expired', value: { ...share, is_expired: true } },
    { state: 'revoked', value: { ...share, status: 'revoked' as const, version: 2 } },
  ])(
    'never displays stale result bodies or export buttons from a $state share response',
    async ({ value }) => {
      server.use(listHandler(value), openHandler({ share: value, job }));
      renderShares();
      await waitFor(() => expect(cache.isFetching()).toBe(0));
      expect(screen.getByRole('button', { name: '查看结果' })).toBeDisabled();
      assertPrivateResultHidden();
    },
  );

  it.each([
    { state: 'expired', value: { ...share, is_expired: true } },
    { state: 'revoked', value: { ...share, status: 'revoked' as const, version: 2 } },
  ])(
    'hides an opened cached result as soon as the refreshed share list reports $state',
    async ({ value }) => {
      renderShares();
      expect(await screen.findByText(PRIVATE_ROW)).toBeInTheDocument();
      server.use(listHandler(value));
      await act(async () => {
        await cache.refetchQueries({ queryKey: ['bi-shares'] });
      });
      await waitFor(() => expect(screen.getByRole('button', { name: '查看结果' })).toBeDisabled());
      assertPrivateResultHidden();
    },
  );

  it.each([
    { state: 'restricted output', value: { ...job, result_access: 'restricted' as const } },
    { state: 'failed run', value: { ...job, status: 'failed' } },
    { state: 'failed node', value: { ...job, nodes: [{ ...job.nodes[0], status: 'failed' }] } },
    {
      state: 'restricted node',
      value: { ...job, nodes: [{ ...job.nodes[0], result_access: 'restricted' as const }] },
    },
  ])(
    'does not render stored rows for $state even if a response still contains an old body',
    async ({ value }) => {
      server.use(openHandler({ share, job: value }));
      renderShares();
      await waitFor(() => expect(cache.isFetching()).toBe(0));
      assertPrivateResultHidden();
    },
  );
});
