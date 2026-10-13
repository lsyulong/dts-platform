import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { useAuthStore } from '../../../stores/auth';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import type { ProductModule } from '../../../product/catalog-types';
import type { ProductResource } from '../../../product/resource-types';
import type { ProductJob } from '../jobs-api';
import SensitiveRecognitionPage from '../SensitiveRecognitionPage';

vi.mock('../../../product/ProductModulePage', () => ({
  default: () => <p>Recognition configuration workspace</p>,
}));
const API = 'http://localhost:8080/api/v1';
const user = {
  id: 'reader',
  tenant_id: 'tenant',
  tenant_name: 'Tenant',
  username: 'reader',
  email: 'reader@example.invalid',
  roles: ['recognition_reader'],
  permissions: [
    'sensitive_data:read',
    'sensitive_data:write',
    'sensitive_data:execute',
    'data_security:all_data',
  ],
};
const module: ProductModule = {
  id: 'recognition-tasks',
  groupId: 'sensitive-data',
  title: { zh: '识别任务', en: 'Recognition tasks' },
  description: { zh: '', en: '' },
  path: '/security/recognition-tasks',
  reference: [],
  implementation: 'new',
  resourceKind: 'recognition_task',
};
const task: ProductResource = {
  id: 'task',
  tenant_id: 'tenant',
  kind: 'recognition_task',
  name: 'Actual published scan',
  code: 'scan',
  description: null,
  parent_id: null,
  status: 'published',
  version: 3,
  properties: { sample_limit: 3 },
  references: [],
  created_by: 'reader',
  created_at: '2026-10-09T10:00:00Z',
  updated_at: '2026-10-09T10:00:00Z',
};
const job: ProductJob = {
  id: 'actual-job',
  source_kind: 'recognition_task',
  source_id: task.id,
  source_version: 3,
  status: 'succeeded',
  cancel_requested: false,
  can_retry: true,
  attempt: 1,
  created_at: task.created_at,
  started_at: task.created_at,
  finished_at: task.created_at,
  error: null,
  result_access: 'authorized',
  nodes: [
    {
      id: 'recognition',
      name: 'Actual sensitive scan',
      status: 'succeeded',
      depends_on: [],
      attempt: 1,
      error: null,
      result_access: 'authorized',
      result: {
        evaluation_scope: 'sample',
        sample_limit: 3,
        sampled_rows: 3,
        columns: [
          {
            column_name: 'customer_email',
            data_type: 'VARCHAR',
            checked_values: 2,
            matched_values: 1,
            null_values: 1,
          },
        ],
        definition_hash: 'a'.repeat(64),
        producer_committed: true,
        finding_ids: ['actual-finding'],
      },
    },
  ],
};
let cache: QueryClient;
function login(permissions = user.permissions) {
  const account = { ...user, permissions };
  localStorage.setItem('token', 'reader-session');
  localStorage.setItem('user', JSON.stringify(account));
  useAuthStore.setState({
    user: account,
    token: 'reader-session',
    initialized: true,
    restoring: false,
    restoreError: null,
  });
}
function handlers(value = job) {
  server.use(
    http.get(`${API}/product/resources/recognition_task`, () =>
      HttpResponse.json({ data: [task], total: 1 }),
    ),
    http.get(`${API}/product/jobs`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('source_kind')).toBe('recognition_task');
      return HttpResponse.json({ data: [value], total: 1 });
    }),
    http.get(`${API}/product/jobs/:id`, () => HttpResponse.json(value)),
  );
}
function renderPage() {
  return renderWithProviders(<SensitiveRecognitionPage module={module} />, {
    route: '/security/recognition-tasks?job=actual-job',
    queryClient: cache,
  });
}
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  cache = createTestQueryClient();
  localStorage.clear();
  login();
  handlers();
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
  vi.restoreAllMocks();
});
describe('Actual sensitive scan execution and protected outputs', () => {
  it('opens actual runs from a real job query and shows committed counts and finding ids', async () => {
    renderPage();
    expect(screen.getByRole('tab', { name: '真实运行' })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByText('实际抽样识别')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'actual-finding' })).toHaveAttribute(
      'href',
      '/security/discovery?finding=actual-finding',
    );
    expect(screen.getByText(/未匹配不代表整表不存在敏感数据/)).toBeInTheDocument();
  });
  it('opens run history without a selected job and never requests an undefined detail', async () => {
    let details = 0;
    server.use(
      http.get(`${API}/product/jobs/:id`, () => {
        details++;
        return HttpResponse.json(job);
      }),
    );
    renderWithProviders(<SensitiveRecognitionPage module={module} />, {
      route: '/security/recognition-tasks',
      queryClient: cache,
    });
    await userEvent.setup().click(screen.getByRole('tab', { name: '真实运行' }));
    await screen.findByRole('button', { name: '查看运行' });
    expect(details).toBe(0);
    expect(screen.queryByText('实际抽样识别')).not.toBeInTheDocument();
  });
  it('does not display an old result body when current job access is restricted', async () => {
    handlers({ ...job, result_access: 'restricted' });
    renderPage();
    await screen.findByText('当前授权或来源已失效，历史计数不可查看。');
    expect(screen.queryByText('实际抽样识别')).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'actual-finding' })).not.toBeInTheDocument();
  });
  it('hides cached counts immediately when a current detail refresh is denied', async () => {
    renderPage();
    await screen.findByText('实际抽样识别');
    server.use(
      http.get(`${API}/product/jobs/:id`, () =>
        HttpResponse.json({ message: 'Recognition proof is no longer valid' }, { status: 403 }),
      ),
    );
    await act(async () => {
      await cache.refetchQueries({ queryKey: ['sensitive-job'] });
    });
    expect(await screen.findByText('Recognition proof is no longer valid')).toBeInTheDocument();
    expect(screen.queryByText('实际抽样识别')).not.toBeInTheDocument();
  });
  it('does not expose stale results to a new permission scope while its request is pending', async () => {
    renderPage();
    await screen.findByText('实际抽样识别');
    let release: () => void = () => {};
    const pause = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/jobs/:id`, async () => {
        await pause;
        return HttpResponse.json({ ...job, result_access: 'restricted' });
      }),
    );
    act(() => login(['sensitive_data:read']));
    expect(screen.queryByText('实际抽样识别')).not.toBeInTheDocument();
    release();
    await screen.findByText('当前授权或来源已失效，历史计数不可查看。');
  });
  it('uses the selected actual task version and preserves the logical idempotency key on network retry', async () => {
    const payloads: unknown[] = [];
    let first = true;
    server.use(
      http.post(
        `${API}/product/resources/recognition_task/:id/actions/execute`,
        async ({ request, params }) => {
          expect(params.id).toBe(task.id);
          const body = await request.json();
          payloads.push(body);
          if (first) {
            first = false;
            return HttpResponse.json({ message: 'Temporary submit failure' }, { status: 503 });
          }
          return HttpResponse.json({ resource: task, run_id: job.id, result: { submitted: true } });
        },
      ),
    );
    renderPage();
    await screen.findByText('实际抽样识别');
    const action = userEvent.setup();
    await action.selectOptions(screen.getByRole('combobox', { name: '识别任务' }), task.id);
    await action.click(screen.getByRole('button', { name: '执行实际识别' }));
    await screen.findByText('Temporary submit failure');
    await action.click(screen.getByRole('button', { name: '执行实际识别' }));
    await screen.findByText('实际运行已提交，等待领取与执行。');
    expect(payloads).toHaveLength(2);
    expect(payloads[0]).toEqual(payloads[1]);
    expect(payloads[0]).toMatchObject({
      expected_version: 3,
      payload: { idempotency_key: expect.any(String) },
    });
  });
  it('never presents uncommitted or failed node results as successful discoveries', async () => {
    handlers({ ...job, nodes: [{ ...job.nodes[0], status: 'failed' }] });
    renderPage();
    await waitFor(() => expect(cache.isFetching()).toBe(0));
    expect(screen.queryByText('实际抽样识别')).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'actual-finding' })).not.toBeInTheDocument();
  });
});
