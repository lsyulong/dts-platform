import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AxiosHeaders, type AxiosResponse } from 'axios';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import client from '../../../api/client';
import type { ProductModule } from '../../../product/catalog-types';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import JobRunsPage from '../JobRunsPage';
import ProductRoute from '../../../product/ProductRoute';
import { getProductModule } from '../../../product/catalog';
import MessagesPage from '../MessagesPage';
import QualityReportPage from '../QualityReportPage';
import SystemContentPage from '../SystemContentPage';
import type { ProductJob } from '../jobs-api';

const API = 'http://localhost:8080/api/v1';
const userA = {
  id: 'actor-a',
  username: 'alice',
  email: 'a@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'Tenant A',
  roles: ['operator'],
  permissions: ['development:read', 'development:execute', 'quality:read'],
};
const userB = { ...userA, id: 'actor-b', tenant_id: 'tenant-b', username: 'bob' };
const module: ProductModule = {
  id: 'messages',
  groupId: 'access',
  path: '/messages',
  title: { zh: '消息', en: 'Messages' },
  description: { zh: '', en: '' },
  reference: [],
  resourceKind: 'message',
  implementation: 'new',
};
const message = {
  id: 'delivery-a',
  source_kind: 'system_message' as const,
  message_id: 'message-a',
  subject: 'Alice private subject',
  body: 'Alice private body',
  status: 'delivered',
  delivered_at: '2026-10-08T10:00:00Z',
  read_at: null,
};
const job: ProductJob = {
  id: 'run-a',
  source_kind: 'development_workflow',
  source_id: 'workflow-a',
  source_version: 4,
  status: 'failed',
  cancel_requested: false,
  can_retry: true,
  attempt: 1,
  created_at: '2026-10-08T10:00:00Z',
  started_at: null,
  finished_at: null,
  error: 'Stored SQL execution error',
  nodes: [
    {
      id: 'query',
      name: 'Stored query',
      status: 'failed',
      depends_on: [],
      attempt: 1,
      result: { rows: [] },
      error: 'DuckDB rejected query',
    },
  ],
};
const emptyReport = {
  status: 'no_results',
  completed_runs: 0,
  failed_runs: 0,
  incomplete_runs: 0,
  checked_rows: 0,
  failed_rows: 0,
  failure_ratio: null,
  counting_unit: 'row_rule_checks',
  window_complete: true,
  empty_checks: 0,
  sampled_checks: 0,
  jobs_scanned: 0,
  total_jobs: 0,
};

function login(user = userA, token = 'session-a') {
  localStorage.setItem('token', token);
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({ token, user, initialized: true, restoring: false, restoreError: null });
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((finish) => {
    resolve = finish;
  });
  return { promise, resolve };
}
function response<T>(data: T): AxiosResponse<T> {
  return {
    data,
    status: 200,
    statusText: 'OK',
    headers: new AxiosHeaders(),
    config: { headers: new AxiosHeaders() },
  };
}
function inboxHandler() {
  return http.get(`${API}/product/messages/inbox`, ({ request }) =>
    HttpResponse.json({
      data: request.headers.get('Authorization') === 'Bearer session-a' ? [message] : [],
      total: request.headers.get('Authorization') === 'Bearer session-a' ? 1 : 0,
    }),
  );
}
function jobsHandler() {
  return http.get(`${API}/product/jobs`, ({ request }) =>
    HttpResponse.json({
      data: request.headers.get('Authorization') === 'Bearer session-a' ? [job] : [],
      total: request.headers.get('Authorization') === 'Bearer session-a' ? 1 : 0,
    }),
  );
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  localStorage.clear();
  login();
  server.use(
    inboxHandler(),
    jobsHandler(),
    http.get(`${API}/product/resources/quality_task`, () =>
      HttpResponse.json({ data: [], total: 0 }),
    ),
  );
});
afterEach(() => {
  server.resetHandlers();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('Real product interactions and session isolation', () => {
  it('marks the real inbox delivery read and applies the unread filter on the server', async () => {
    let read = false;
    const filters: string[] = [];
    server.use(
      http.get(`${API}/product/messages/inbox`, ({ request }) => {
        const url = new URL(request.url);
        expect(request.headers.get('Authorization')).toBe('Bearer session-a');
        expect(url.searchParams.get('limit')).toBe('20');
        filters.push(url.searchParams.get('unread_only') || '');
        const visible = !read || url.searchParams.get('unread_only') !== 'true';
        return HttpResponse.json({
          data: visible ? [{ ...message, read_at: read ? '2026-10-08T11:00:00Z' : null }] : [],
          total: visible ? 1 : 0,
        });
      }),
      http.post(`${API}/product/messages/delivery-a/read`, ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer session-a');
        read = true;
        return HttpResponse.json({ ...message, read_at: '2026-10-08T11:00:00Z' });
      }),
    );
    renderWithProviders(<MessagesPage module={module} />);
    const user = userEvent.setup();
    await screen.findByRole('button', { name: /Alice private subject/ });
    await user.click(screen.getByLabelText('只看未读'));
    await user.click(await screen.findByRole('button', { name: '标为已读' }));
    expect(await screen.findByText('没有收到符合筛选条件的消息')).toBeInTheDocument();
    expect(filters).toContain('true');
    expect(read).toBe(true);
  });

  it('does not reopen message details when a read request completes after closing', async () => {
    const pending = deferred<AxiosResponse<typeof message>>();
    vi.spyOn(client, 'post').mockImplementationOnce(() => pending.promise);
    renderWithProviders(<MessagesPage module={module} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Alice private subject/ }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: '标为已读' }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
    await act(async () => {
      pending.resolve(response(message));
      await pending.promise;
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('clears old inbox details and ignores the previous actor read response on identity change', async () => {
    const pending = deferred<AxiosResponse<typeof message>>();
    vi.spyOn(client, 'post').mockImplementationOnce(() => pending.promise);
    renderWithProviders(<MessagesPage module={module} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Alice private subject/ }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: '标为已读' }));
    await act(async () => {
      login(userB, 'session-b');
      pending.resolve(response(message));
      await pending.promise;
    });
    expect(await screen.findByText('没有收到符合筛选条件的消息')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByText('Alice private body')).not.toBeInTheDocument();
  });

  it('fetches actual run details and never offers execution to a read-only actor', async () => {
    login({ ...userA, permissions: ['development:read'] });
    const request = vi.fn();
    server.use(
      http.get(`${API}/product/jobs/run-a`, ({ request: req }) => {
        request(req.headers.get('Authorization'));
        return HttpResponse.json({
          ...job,
          error: 'Fresh persisted error',
          nodes: [{ ...job.nodes[0], result: { rows: [{ actual: 42 }] } }],
        });
      }),
    );
    renderWithProviders(<JobRunsPage />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'run-a' }));
    expect(await screen.findByText('Fresh persisted error')).toBeInTheDocument();
    expect(screen.getByText(/"actual": 42/)).toBeInTheDocument();
    expect(request).toHaveBeenCalledWith('Bearer session-a');
    expect(screen.queryByRole('button', { name: '重试' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '取消' })).not.toBeInTheDocument();
    await act(async () => {
      login({ ...userA, permissions: [] });
    });
    expect(screen.getByText('当前账号没有查看权限')).toBeInTheDocument();
    expect(screen.queryByText('run-a')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('loads actual SQL job instances on development task operations using development permissions', async () => {
    const requests = vi.fn();
    server.use(
      http.get(`${API}/product/jobs`, ({ request }) => {
        const kind = new URL(request.url).searchParams.get('source_kind');
        requests(kind);
        return HttpResponse.json({
          data:
            kind === 'sql_document'
              ? [{ ...job, source_kind: 'sql_document', source_id: 'sql-a' }]
              : [],
          total: kind === 'sql_document' ? 1 : 0,
        });
      }),
    );
    const module = getProductModule('development-task-operations');
    if (!module) throw new Error('Development task operations module is required');
    expect(module.permission).toBe('development:read');
    renderWithProviders(<ProductRoute module={module} />);
    expect(await screen.findByRole('button', { name: 'run-a' })).toBeInTheDocument();
    expect(requests).toHaveBeenCalledWith('sql_document');
    expect(screen.getByRole('heading', { name: '任务运维' })).toBeInTheDocument();
    expect(screen.queryByText('当前账号没有查看权限')).not.toBeInTheDocument();
  });

  it('opens an actual run from its link and does not use list results when the detail request is denied', async () => {
    const reads = vi.fn();
    server.use(
      http.get(`${API}/product/jobs/run-a`, ({ request }) => {
        reads(request.headers.get('Authorization'));
        return HttpResponse.json({ message: '该运行结果的授权已撤销' }, { status: 403 });
      }),
    );
    renderWithProviders(<JobRunsPage />, { route: '/development/workflow-runs?job=run-a' });
    const dialog = await screen.findByRole('dialog');
    expect(await within(dialog).findByText('该运行结果的授权已撤销')).toBeInTheDocument();
    expect(reads).toHaveBeenCalledWith('Bearer session-a');
    expect(within(dialog).queryByText('Stored query')).not.toBeInTheDocument();
    expect(within(dialog).queryByText('Stored SQL execution error')).not.toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('retains the same retry idempotency key after an actual request failure', async () => {
    const keys: unknown[] = [];
    server.use(
      http.post(`${API}/product/jobs/run-a/retry`, async ({ request }) => {
        keys.push(((await request.json()) as { idempotency_key: string }).idempotency_key);
        return keys.length === 1
          ? HttpResponse.json({ message: '暂时无法确认，请重试' }, { status: 503 })
          : HttpResponse.json(
              { job: { ...job, id: 'new-run', status: 'queued' }, created: true },
              { status: 202 },
            );
      }),
    );
    renderWithProviders(<JobRunsPage />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: '重试' }));
    await user.click(screen.getByRole('button', { name: '确认' }));
    expect(await screen.findByText('暂时无法确认，请重试')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '返回' }));
    await user.click(screen.getByRole('button', { name: '重试' }));
    await user.click(screen.getByRole('button', { name: '确认' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(keys).toHaveLength(2);
    expect(keys[0]).toEqual(keys[1]);
    expect(keys[0]).toEqual(expect.any(String));
    const row = screen.getByRole('button', { name: 'run-a' }).closest('tr');
    expect(row).toHaveTextContent('失败');
    expect(row).not.toHaveTextContent('成功');
  });

  it('removes a pending job action across actor changes and ignores its late completion', async () => {
    const pending = deferred<AxiosResponse<unknown>>();
    vi.spyOn(client, 'post').mockImplementationOnce(() => pending.promise);
    renderWithProviders(<JobRunsPage />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: '重试' }));
    await user.click(screen.getByRole('button', { name: '确认' }));
    await act(async () => {
      login(userB, 'session-b');
      pending.resolve(response({ job, created: true }));
      await pending.promise;
    });
    expect(await screen.findByText('没有符合条件的实际运行记录')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'run-a' })).not.toBeInTheDocument();
  });

  it('shows missing quality evidence and exports the generated range after input changes', async () => {
    let periodEnd = '';
    server.use(
      http.post(`${API}/product/quality/report`, async ({ request }) => {
        periodEnd = ((await request.json()) as { period_end: string }).period_end;
        return HttpResponse.json(emptyReport);
      }),
    );
    vi.stubGlobal(
      'URL',
      class extends URL {
        static createObjectURL = vi.fn(() => 'blob:quality-report');
        static revokeObjectURL = vi.fn();
      },
    );
    const downloads: string[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      downloads.push(this.download);
    });
    renderWithProviders(<QualityReportPage />);
    const user = userEvent.setup();
    const start = screen.getByLabelText('开始日期');
    const end = screen.getByLabelText('结束日期');
    await user.clear(start);
    await user.type(start, '2026-10-01');
    await user.clear(end);
    await user.type(end, '2026-10-08');
    await user.click(screen.getByRole('button', { name: '生成报告' }));
    expect(await screen.findByText('报告结果 · 暂无校验结果')).toBeInTheDocument();
    expect(screen.getByText('缺少实际校验数据，不能据此判定质量通过。')).toBeInTheDocument();
    expect(screen.queryByText('0.00%')).not.toBeInTheDocument();
    const exclusiveEnd = new Date('2026-10-09T00:00:00').toISOString();
    expect(periodEnd).toBe(exclusiveEnd);
    await user.clear(start);
    await user.type(start, '2026-10-02');
    await user.click(screen.getByRole('button', { name: '导出报告' }));
    expect(downloads).toEqual(['quality-report-2026-10-01-2026-10-08.json']);
  });

  it('resets report loading on identity changes and rejects the old generated result', async () => {
    const pending = deferred<AxiosResponse<typeof emptyReport>>();
    vi.spyOn(client, 'post').mockImplementationOnce(() => pending.promise);
    renderWithProviders(<QualityReportPage />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: '生成报告' }));
    expect(screen.getByRole('button', { name: '正在生成' })).toBeDisabled();
    await act(async () => {
      login(userB, 'session-b');
      pending.resolve(response(emptyReport));
      await pending.promise;
    });
    expect(screen.getByRole('button', { name: '生成报告' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: '导出报告' })).not.toBeInTheDocument();
  });

  it('warns when the report window is incomplete instead of implying a passing result', async () => {
    server.use(
      http.post(`${API}/product/quality/report`, () =>
        HttpResponse.json({
          ...emptyReport,
          status: 'incomplete_window',
          window_complete: false,
          jobs_scanned: 200,
          total_jobs: 250,
        }),
      ),
    );
    renderWithProviders(<QualityReportPage />);
    await userEvent.setup().click(screen.getByRole('button', { name: '生成报告' }));
    expect(
      await screen.findByText('历史数据超过本次扫描范围，报告不完整，不能据此判定质量通过。'),
    ).toBeInTheDocument();
    expect(screen.getByText('200 / 250')).toBeInTheDocument();
    expect(screen.queryByText('报告结果 · 校验通过')).not.toBeInTheDocument();
  });

  it('renders only real published authorized content and clears it across tenants', async () => {
    const announcement = { ...module, id: 'announcements', resourceKind: 'announcement' };
    const body = '<script>alert("plain-content")</script>';
    server.use(
      http.get(`${API}/product/content/announcement`, ({ request }) =>
        HttpResponse.json({
          data:
            request.headers.get('Authorization') === 'Bearer session-a'
              ? [
                  {
                    id: 'ann-a',
                    name: 'Authorized announcement',
                    updated_at: '2026-10-08T10:00:00Z',
                    properties: { content: body },
                  },
                ]
              : [],
        }),
      ),
    );
    renderWithProviders(<SystemContentPage module={announcement} />);
    expect(await screen.findByText(body)).toBeInTheDocument();
    expect(document.querySelector('script')).toBeNull();
    expect(screen.queryByRole('button', { name: '配置与发布' })).not.toBeInTheDocument();
    await act(async () => {
      login(userB, 'session-b');
    });
    expect(await screen.findByText('当前没有在有效期内且对您授权发布的内容')).toBeInTheDocument();
    expect(screen.queryByText(body)).not.toBeInTheDocument();
  });
});
