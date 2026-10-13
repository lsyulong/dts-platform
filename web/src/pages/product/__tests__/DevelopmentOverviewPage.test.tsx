import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import DevelopmentOverviewPage from '../DevelopmentOverviewPage';
import ProductRoute from '../../../product/ProductRoute';
import { getProductModule } from '../../../product/catalog';

const API = 'http://localhost:8080/api/v1';
const user = {
  id: 'operator',
  username: 'operator',
  email: 'operator@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'Tenant A',
  roles: ['developer'],
  permissions: ['development:read'],
};
const run = (kind: string, id: string, status = 'failed') => ({
  id,
  source_kind: kind,
  source_id: `${kind}-source`,
  source_version: 3,
  status,
  cancel_requested: false,
  can_retry: true,
  attempt: 1,
  created_at: kind === 'sql_document' ? '2026-10-09T12:00:00Z' : '2026-10-09T11:00:00Z',
  started_at: null,
  finished_at: null,
  nodes: [],
  error: status === 'failed' ? 'query failed' : null,
});

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  localStorage.setItem('token', 'overview-session');
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({
    token: 'overview-session',
    user,
    initialized: true,
    restoring: false,
    restoreError: null,
  });
});
afterEach(() => {
  server.resetHandlers();
  localStorage.clear();
});

describe('Development operations overview', () => {
  it('combines actual SQL and workflow runs, sends the filter, and links to real run detail', async () => {
    const requests: string[] = [];
    server.use(
      http.get(`${API}/product/jobs`, ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer overview-session');
        const url = new URL(request.url),
          kind = url.searchParams.get('source_kind')!;
        requests.push(`${kind}:${url.searchParams.get('status') ?? ''}`);
        return HttpResponse.json({
          data: [run(kind, kind === 'sql_document' ? 'sql-run' : 'workflow-run')],
          total: 1,
        });
      }),
    );
    const overview = getProductModule('development-overview');
    expect(overview?.permission).toBe('development:read');
    renderWithProviders(<ProductRoute module={overview!} />);
    expect(await screen.findByText('2', { selector: 'strong' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'sql-run' })).toHaveAttribute(
      'href',
      '/development/task-runs?job=sql-run',
    );
    expect(screen.getByRole('link', { name: 'workflow-run' })).toHaveAttribute(
      'href',
      '/development/workflow-runs?job=workflow-run',
    );
    await userEvent.selectOptions(screen.getByLabelText('运行状态'), 'failed');
    await waitFor(() =>
      expect(requests).toEqual(
        expect.arrayContaining(['sql_document:failed', 'development_workflow:failed']),
      ),
    );
  });

  it('fails closed when one source returns another source kind instead of showing a false zero', async () => {
    server.use(
      http.get(`${API}/product/jobs`, ({ request }) => {
        const kind = new URL(request.url).searchParams.get('source_kind')!;
        return HttpResponse.json({
          data: [run(kind === 'sql_document' ? 'development_workflow' : kind, `${kind}-run`)],
          total: 1,
        });
      }),
    );
    renderWithProviders(<DevelopmentOverviewPage />);
    expect(await screen.findByText('运行响应与实际来源不一致，请刷新后重试')).toBeInTheDocument();
    expect(
      screen.getByText(
        (_, node) => node?.tagName === 'SPAN' && node.textContent === '符合筛选的实际实例：—',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'sql_document-run' })).not.toBeInTheDocument();
  });
});
