import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { beforeAll, afterAll, afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { server } from '../../../test/mocks/server';
import { renderWithProviders } from '../../../test/test-utils';
import { useAuthStore } from '../../../stores/auth';
import QualityMonitorPage from '../QualityMonitorPage';
import QualityAlertPanel, { QualityObservationSummary } from '../QualityAlertPanel';
import type { QualityAlertView, QualityObservation } from '../quality-alert-types';
import type { ProductModule } from '../../../product/catalog-types';

vi.mock('../../../product/ProductModulePage', () => ({
  default: () => <p>Monitor definitions</p>,
}));
const API = 'http://localhost:8080/api/v1';
const account = {
  id: 'quality-user',
  username: 'quality',
  email: 'q@test',
  tenant_id: 'tenant-a',
  tenant_name: 'A',
  roles: ['member'],
  permissions: ['quality:read', 'quality:write'],
};
const module: ProductModule = {
  id: 'quality-monitoring',
  groupId: 'quality',
  title: { zh: '质量监控', en: 'Quality monitoring' },
  description: { zh: '质量', en: 'Quality' },
  path: '/quality/monitoring',
  reference: [],
  resourceKind: 'quality_monitor',
  implementation: 'new',
};
const monitor = {
  id: 'monitor-a',
  kind: 'quality_monitor',
  tenant_id: account.tenant_id,
  code: 'orders_quality',
  name: '订单质量监控',
  version: 3,
  properties: { object_id: 'object-a', failure_threshold: 0.1, window_hours: 24 },
  status: 'draft',
  references: [],
};
const observation: QualityObservation = {
  id: 'observation-a',
  monitor_id: monitor.id,
  monitor_version: 3,
  object_id: 'object-a',
  actor_id: account.id,
  period_start: '2026-10-08T00:00:00Z',
  period_end: '2026-10-09T00:00:00Z',
  state: 'unavailable',
  reason: 'no_results',
  summary: {
    completed_runs: 0,
    failed_runs: 0,
    incomplete_runs: 0,
    checked_rows: 0,
    failed_rows: 0,
    failure_ratio: null,
    threshold: 0.1,
    threshold_exceeded: null,
    window_complete: true,
    sampled_checks: 0,
    counting_unit: 'row_rule_checks',
  },
  result_access: 'allowed',
  dependencies: [{ kind: 'quality_monitor', id: monitor.id, version: 3 }],
  run_ids: [],
  created_at: '2026-10-09T00:00:00Z',
};
const record: QualityAlertView = {
  observation: {
    ...observation,
    state: 'threshold_exceeded',
    reason: 'threshold_exceeded',
    run_ids: ['actual-job'],
    summary: {
      ...observation.summary,
      completed_runs: 1,
      checked_rows: 100,
      failed_rows: 25,
      failure_ratio: 0.25,
      threshold_exceeded: true,
    },
  },
  alert: {
    id: 'alert-a',
    tenant_id: account.tenant_id,
    monitor_id: monitor.id,
    monitor_version: 3,
    status: 'open',
    version: 1,
    opening_observation_id: 'observation-a',
    last_observation_id: 'observation-a',
    acknowledged_by: null,
    acknowledged_at: null,
    resolved_at: null,
    created_at: observation.created_at,
    updated_at: observation.created_at,
  },
};
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  useAuthStore.setState({ token: 'quality-token', user: account });
  server.use(
    http.get(`${API}/product/resources/quality_monitor`, () =>
      HttpResponse.json({ data: [monitor], total: 1 }),
    ),
    http.get(`${API}/product/quality-monitors/monitor-a/observations`, () =>
      HttpResponse.json({ data: [], total: 0 }),
    ),
    http.get(`${API}/product/quality-monitors/monitor-a/alerts`, () =>
      HttpResponse.json({ data: [record], total: 1 }),
    ),
    http.get(`${API}/product/quality-alerts/alert-a/events`, () =>
      HttpResponse.json({ data: [], total: 0 }),
    ),
  );
});
afterEach(() => {
  server.resetHandlers();
  vi.restoreAllMocks();
});

describe('Actual quality observations and alerts', () => {
  it('keeps the same evaluation UUID after failure and waits for a persisted server observation', async () => {
    const bodies: unknown[] = [];
    let recorded = false;
    server.use(
      http.post(`${API}/product/quality-monitors/monitor-a/evaluate`, async ({ request }) => {
        bodies.push(await request.json());
        if (bodies.length === 1)
          return HttpResponse.json({ message: '真实仓储暂不可用' }, { status: 503 });
        recorded = true;
        return HttpResponse.json({ observation, alert: null, replayed: false });
      }),
      http.get(`${API}/product/quality-monitors/monitor-a/observations`, () =>
        HttpResponse.json({ data: recorded ? [observation] : [], total: recorded ? 1 : 0 }),
      ),
    );
    renderWithProviders(<QualityMonitorPage module={module} />);
    await screen.findByRole('option', { name: /订单质量监控/ });
    await userEvent.selectOptions(screen.getByLabelText('当前页监控'), monitor.id);
    await screen.findByText(/暂无持久化观察/);
    await userEvent.click(screen.getByRole('button', { name: '评估并记录' }));
    await screen.findByText('真实仓储暂不可用');
    expect(screen.queryByText('窗口内暂无真实运行结果')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: '评估并记录' }));
    await screen.findByText('窗口内暂无真实运行结果');
    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toEqual(bodies[0]);
    expect(bodies[0]).toMatchObject({ expected_version: 3 });
    expect(screen.getByText('暂无可用比例')).toBeInTheDocument();
    expect(screen.queryByText('0.00%')).not.toBeInTheDocument();
    expect(screen.queryByText('已恢复')).not.toBeInTheDocument();
  });

  it('never keeps old observed counts visible when a fresh authorization check fails', async () => {
    let denied = false;
    server.use(
      http.get(`${API}/product/quality-monitors/monitor-a/observations`, () =>
        denied
          ? HttpResponse.json({ message: '当前数据授权已撤销' }, { status: 403 })
          : HttpResponse.json({ data: [record.observation], total: 1 }),
      ),
    );
    renderWithProviders(<QualityMonitorPage module={module} />);
    await screen.findByRole('option', { name: /订单质量监控/ });
    await userEvent.selectOptions(screen.getByLabelText('当前页监控'), monitor.id);
    await screen.findByText('25.00%');
    denied = true;
    await userEvent.click(screen.getByRole('button', { name: '重新核验' }));
    await screen.findByText('当前数据授权已撤销');
    expect(screen.queryByText('25.00%')).not.toBeInTheDocument();
  });

  it('uses server status filters and displays real acknowledgments without resolving locally', async () => {
    const statuses: (string | null)[] = [];
    server.use(
      http.get(`${API}/product/quality-monitors/monitor-a/alerts`, ({ request }) => {
        statuses.push(new URL(request.url).searchParams.get('status'));
        return HttpResponse.json({ data: [record], total: 1 });
      }),
    );
    renderWithProviders(<QualityMonitorPage module={module} />);
    await screen.findByRole('option', { name: /订单质量监控/ });
    await userEvent.selectOptions(screen.getByLabelText('当前页监控'), monitor.id);
    await userEvent.click(screen.getByRole('button', { name: '告警实例' }));
    await userEvent.selectOptions(screen.getByLabelText('告警状态'), 'open');
    await waitFor(() => expect(statuses).toContain('open'));
    await userEvent.click(await screen.findByRole('button', { name: /待处理.*v1/ }));
    await screen.findByText('25.00%');
    expect(screen.queryByRole('button', { name: '恢复' })).not.toBeInTheDocument();
  });

  it('acknowledges the same version with a stable UUID after failure and does not fake a successful status', async () => {
    const invoke = vi
      .fn()
      .mockRejectedValueOnce(new Error('无法提交处理记录'))
      .mockResolvedValueOnce(undefined);
    renderWithProviders(<QualityAlertPanel record={record} onAcknowledge={invoke} />);
    await userEvent.type(screen.getByLabelText('处理说明'), '检查上游空值');
    await userEvent.click(screen.getByRole('button', { name: '确认处理' }));
    await screen.findByText('无法提交处理记录');
    expect(screen.queryByText('已确认')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: '确认处理' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    expect(invoke.mock.calls[1][0]).toEqual(invoke.mock.calls[0][0]);
    expect(invoke.mock.calls[0][0]).toMatchObject({ expected_version: 1, reason: '检查上游空值' });
    expect(screen.getByText('待处理')).toBeInTheDocument();
  });

  it('shows self notices with a real pending state and marks read through the self API without user IDs', async () => {
    useAuthStore.setState({ user: { ...account, permissions: [] } });
    let read = false;
    const payloads: unknown[] = [];
    let query = '';
    server.use(
      http.get(`${API}/product/quality-notices`, ({ request }) => {
        query = new URL(request.url).search;
        return HttpResponse.json({
          data: read
            ? []
            : [
                {
                  id: 'notice-a',
                  alert_id: record.alert.id,
                  recipient_id: account.id,
                  subject: '订单质量提醒',
                  link: '/quality/monitoring',
                  delivery_status: 'pending',
                  read_at: null,
                  created_at: observation.created_at,
                },
              ],
          total: read ? 0 : 1,
        });
      }),
      http.post(`${API}/product/quality-notices/notice-a/read`, async ({ request }) => {
        payloads.push(await request.json());
        read = true;
        return HttpResponse.json({ read_at: observation.created_at });
      }),
    );
    renderWithProviders(<QualityMonitorPage module={module} />);
    await userEvent.click(screen.getByRole('tab', { name: '我的未读提醒' }));
    await screen.findByText('订单质量提醒');
    expect(screen.getByText(/发件箱待投递/)).toBeInTheDocument();
    expect(screen.queryByText('实际已投递')).not.toBeInTheDocument();
    expect(query).not.toContain('user_id');
    expect(query).toContain('unread_only=true');
    await userEvent.click(screen.getByRole('button', { name: '标为已读' }));
    await screen.findByText('暂无本人提醒');
    expect(payloads).toEqual([{}]);
  });

  it('hides all counts on permission loss and preserves unavailable/null instead of a passing score', async () => {
    renderWithProviders(<QualityMonitorPage module={module} />);
    await screen.findByRole('option', { name: /订单质量监控/ });
    await userEvent.selectOptions(screen.getByLabelText('当前页监控'), monitor.id);
    await userEvent.click(screen.getByRole('button', { name: '告警实例' }));
    await userEvent.click(await screen.findByRole('button', { name: /待处理.*v1/ }));
    await screen.findByText('25.00%');
    act(() => useAuthStore.setState({ user: { ...account, permissions: [] } }));
    await screen.findByText('当前账号没有质量查看权限');
    expect(screen.queryByText('25.00%')).not.toBeInTheDocument();
  });

  it('does not render protected values for a restricted observation', () => {
    renderWithProviders(
      <QualityObservationSummary
        observation={{ ...record.observation, result_access: 'restricted' }}
      />,
    );
    expect(screen.getByText('当前数据授权不可读取观察计数')).toBeInTheDocument();
    expect(screen.queryByText('25')).not.toBeInTheDocument();
    expect(screen.queryByText('25.00%')).not.toBeInTheDocument();
  });
});
