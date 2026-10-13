import { useState } from 'react';
import { act, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import type { ProductResource } from '../../../product/resource-types';
import type { ProductModule } from '../../../product/catalog-types';
import MetricRunControls from '../../../product/MetricRunControls';
import MetricsPage from '../MetricsPage';

vi.mock('../../../product/ProductModulePage', () => ({ default: () => <p>Definition panel</p> }));
const API = 'http://localhost:8080/api/v1';
const account = {
  id: 'metric-user',
  username: 'metrics',
  email: 'm@example.invalid',
  tenant_id: 'metrics-tenant',
  tenant_name: 'Metrics',
  roles: ['member'],
  permissions: ['metrics:read', 'metrics:execute'],
};
const module: ProductModule = {
  id: 'atomic-metrics',
  groupId: 'metrics',
  title: { zh: '原子指标', en: 'Atomic metrics' },
  description: { zh: '指标', en: 'Metrics' },
  path: '/metrics/atomic',
  reference: [],
  resourceKind: 'atomic_metric',
  implementation: 'new',
};
const record: ProductResource = {
  id: 'metric-a',
  tenant_id: account.tenant_id,
  kind: 'atomic_metric',
  code: 'revenue',
  name: '销售金额',
  description: null,
  parent_id: null,
  status: 'published',
  properties: {},
  references: [],
  version: 2,
  created_by: account.id,
  created_at: '2026-10-09T00:00:00Z',
  updated_at: '2026-10-09T00:00:00Z',
};
const run = {
  id: 'metric-job',
  source_kind: 'atomic_metric',
  source_id: record.id,
  source_version: 2,
  status: 'succeeded',
  cancel_requested: false,
  can_retry: true,
  attempt: 1,
  created_at: '2026-10-09T00:00:00Z',
  started_at: '2026-10-09T00:00:01Z',
  finished_at: '2026-10-09T00:00:02Z',
  error: null,
  nodes: [
    {
      id: 'metric',
      name: '执行真实指标聚合',
      status: 'succeeded',
      depends_on: [],
      attempt: 1,
      result: null,
      error: null,
      result_access: 'summary',
    },
  ],
};
function login(permissions = account.permissions) {
  const user = { ...account, permissions };
  localStorage.setItem('token', 'metric-token');
  localStorage.setItem('user', JSON.stringify(user));
  useAuthStore.setState({
    user,
    token: 'metric-token',
    initialized: true,
    restoring: false,
    restoreError: null,
  });
}
function Controls({
  source = record,
  initial = { idempotency_key: 'stable-request' },
}: {
  source?: ProductResource;
  initial?: Record<string, unknown>;
}) {
  const [payload, setPayload] = useState(initial);
  return (
    <form data-testid="window-form">
      <MetricRunControls record={source} payload={payload} onChange={setPayload} />
      <output data-testid="payload">{JSON.stringify(payload)}</output>
    </form>
  );
}
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => login());
afterEach(() => {
  cleanup();
  server.resetHandlers();
  localStorage.clear();
  useAuthStore.setState({ user: null, token: null });
});

describe('MetricRunControls', () => {
  it('uses explicit UTC bounds, removes as_of and preserves the stable key', () => {
    renderWithProviders(
      <Controls initial={{ idempotency_key: 'stable-request', as_of: '2026-01-01T00:00:00Z' }} />,
    );
    fireEvent.change(screen.getByLabelText(/窗口起点/), {
      target: { value: '2026-09-01T12:34:56' },
    });
    fireEvent.change(screen.getByLabelText(/窗口终点/), {
      target: { value: '2026-10-01T12:34:56' },
    });
    expect(JSON.parse(screen.getByTestId('payload').textContent!)).toEqual({
      idempotency_key: 'stable-request',
      window_start: '2026-09-01T12:34:56.000Z',
      window_end: '2026-10-01T12:34:56.000Z',
    });
    expect(screen.queryByLabelText(/截至时间 as_of/)).not.toBeInTheDocument();
  });
  it('uses frozen composite periods without inventing a new as_of or window', () => {
    renderWithProviders(
      <Controls
        source={{
          ...record,
          kind: 'composite_metric',
          properties: {
            publication: {
              plan: {
                time_timezone: 'Asia/Shanghai',
                measures: [{ period: { unit: 'month', length: 1, timezone: 'Asia/Shanghai' } }],
              },
              references: [
                { code: 'monthly_revenue', kind: 'derived_metric', id: 'monthly', version: 4 },
              ],
            },
          },
        }}
      />,
    );
    expect(screen.getByLabelText(/截至时间 as_of/)).toHaveValue('');
    expect(screen.queryByLabelText(/窗口起点/)).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/截至时间 as_of/), {
      target: { value: '2026-10-09T00:00:00' },
    });
    expect(JSON.parse(screen.getByTestId('payload').textContent!)).toEqual({
      idempotency_key: 'stable-request',
      as_of: '2026-10-09T00:00:00.000Z',
    });
    expect(screen.getByText(/monthly_revenue · derived_metric · v4/)).toBeInTheDocument();
  });
  it('unknown composite mode is explicit and switching removes conflicting bounds', async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <Controls
        source={{ ...record, kind: 'composite_metric' }}
        initial={{
          idempotency_key: 'stable-request',
          window_start: '2026-09-01T00:00:00Z',
          window_end: '2026-10-01T00:00:00Z',
        }}
      />,
    );
    await user.selectOptions(screen.getByLabelText(/窗口模式/), 'calendar');
    expect(JSON.parse(screen.getByTestId('payload').textContent!)).toEqual({
      idempotency_key: 'stable-request',
    });
    expect(screen.getByLabelText(/截至时间 as_of/)).toBeRequired();
    expect(screen.getByText(/当前没有完整冻结周期证明/)).toBeInTheDocument();
  });
  it('invalid and unbounded windows block native form submission', () => {
    renderWithProviders(
      <Controls
        initial={{ window_start: '2026-10-01T00:00:00Z', window_end: '2026-09-01T00:00:00Z' }}
      />,
    );
    expect(screen.getByTestId('window-form')).toBeInvalid();
    expect(screen.getByRole('alert')).toHaveTextContent('窗口终点须晚于起点');
    fireEvent.change(screen.getByLabelText(/窗口终点/), {
      target: { value: '2040-10-01T00:00:00' },
    });
    expect(screen.getByTestId('window-form')).toBeInvalid();
  });
});

async function openRuns() {
  const user = userEvent.setup();
  renderWithProviders(<MetricsPage module={module} />);
  await user.click(screen.getByRole('tab', { name: '实际计算记录' }));
  return user;
}
describe('MetricsPage observed runs', () => {
  it('filters all three actual sources and paginates using the server total', async () => {
    const requests: URL[] = [];
    server.use(
      http.get(`${API}/product/jobs`, ({ request }) => {
        const url = new URL(request.url);
        requests.push(url);
        return HttpResponse.json({
          data: [
            {
              ...run,
              id: `run-${url.searchParams.get('offset')}`,
              source_kind: url.searchParams.get('source_kind'),
            },
          ],
          total: 21,
        });
      }),
    );
    const user = await openRuns();
    await screen.findByRole('button', { name: 'run-0' });
    await user.click(screen.getByRole('button', { name: '下一页' }));
    await screen.findByRole('button', { name: 'run-20' });
    expect(requests.at(-1)?.searchParams.get('offset')).toBe('20');
    await user.selectOptions(screen.getByLabelText('指标类型'), 'derived_metric');
    await waitFor(() =>
      expect(requests.at(-1)?.searchParams.get('source_kind')).toBe('derived_metric'),
    );
    expect(requests.at(-1)?.searchParams.get('offset')).toBe('0');
    await user.selectOptions(screen.getByLabelText('指标类型'), 'composite_metric');
    await waitFor(() =>
      expect(requests.at(-1)?.searchParams.get('source_kind')).toBe('composite_metric'),
    );
    await user.selectOptions(screen.getByLabelText('运行状态'), 'failed');
    await waitFor(() => expect(requests.at(-1)?.searchParams.get('status')).toBe('failed'));
    expect(requests.at(-1)?.searchParams.get('limit')).toBe('20');
  });
  it('never uses list raw rows as a result and respects restricted detail access', async () => {
    server.use(
      http.get(`${API}/product/jobs`, () =>
        HttpResponse.json({
          data: [
            { ...run, nodes: [{ ...run.nodes[0], result: { rows: [{ secret: 'LIST_SECRET' }] } }] },
          ],
          total: 1,
        }),
      ),
      http.get(`${API}/product/jobs/${run.id}`, () =>
        HttpResponse.json({
          ...run,
          nodes: [{ ...run.nodes[0], result: null, result_access: 'restricted' }],
        }),
      ),
    );
    const user = await openRuns();
    await user.click(await screen.findByRole('button', { name: run.id }));
    expect(await screen.findByText(/当前授权、发布依赖或物理元数据证明已失效/)).toBeInTheDocument();
    expect(screen.queryByText('LIST_SECRET')).not.toBeInTheDocument();
  });
  it('preserves authorized Decimal text and NULL then clears details on permission loss', async () => {
    const result = {
      executor: 'metric_aggregate_v1',
      metric_version: 2,
      model_version: 3,
      metadata_version: 1,
      grain_columns: ['id'],
      window: { start: '2026-09-01T00:00:00Z', end: '2026-10-01T00:00:00Z', timezone: 'UTC' },
      numeric_semantics: 'engine_native_exact',
      columns: [
        { name: 'metric_value', data_type: 'Decimal128(38,2)', is_numeric: true },
        { name: 'input_rows', data_type: 'Int64', is_numeric: true },
      ],
      rows: [
        { metric_value: '90071992547409931234.56', input_rows: '9007199254740993' },
        { metric_value: null, input_rows: 0 },
      ],
      row_count: 2,
      suggested_chart: null,
    };
    server.use(
      http.get(`${API}/product/jobs`, () => HttpResponse.json({ data: [run], total: 1 })),
      http.get(`${API}/product/jobs/${run.id}`, () =>
        HttpResponse.json({
          ...run,
          nodes: [{ ...run.nodes[0], result, result_access: 'authorized' }],
        }),
      ),
    );
    const user = await openRuns();
    await user.click(await screen.findByRole('button', { name: run.id }));
    expect(await screen.findByText('90071992547409931234.56')).toBeInTheDocument();
    expect(screen.getByText('9007199254740993')).toBeInTheDocument();
    expect(screen.getByText('NULL')).toBeInTheDocument();
    act(() => login([]));
    expect(screen.queryByText('90071992547409931234.56')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('failed retries keep one UUID and do not manufacture queued or calculated results', async () => {
    const keys: unknown[] = [];
    server.use(
      http.get(`${API}/product/jobs`, () =>
        HttpResponse.json({ data: [{ ...run, status: 'failed' }], total: 1 }),
      ),
      http.post(`${API}/product/jobs/${run.id}/retry`, async ({ request }) => {
        keys.push(((await request.json()) as { idempotency_key: string }).idempotency_key);
        return HttpResponse.json({ message: '真实版本已失效' }, { status: 409 });
      }),
    );
    const user = await openRuns();
    await user.click(await screen.findByRole('button', { name: '重试计算' }));
    await user.click(screen.getByRole('button', { name: '确认操作' }));
    await screen.findByText('真实版本已失效');
    await user.click(screen.getByRole('button', { name: '关闭' }));
    await user.click(screen.getByRole('button', { name: '重试计算' }));
    await user.click(screen.getByRole('button', { name: '确认操作' }));
    await waitFor(() => expect(keys).toHaveLength(2));
    expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/);
    expect(keys[1]).toBe(keys[0]);
    expect(screen.queryByText('已排队')).not.toBeInTheDocument();
  });
});
