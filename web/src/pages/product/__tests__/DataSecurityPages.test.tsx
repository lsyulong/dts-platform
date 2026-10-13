import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AxiosHeaders, type AxiosResponse } from 'axios';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import client from '../../../api/client';
import { getProductModuleByPath } from '../../../product/catalog';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import DataAccessApprovalsPage from '../DataAccessApprovalsPage';
import MyDataAccessPage from '../MyDataAccessPage';
import DataAccessAuditPage from '../DataAccessAuditPage';
import JobNodeResult from '../JobNodeResult';

const API = 'http://localhost:8080/api/v1';
const user = {
  id: 'reviewer',
  username: 'reviewer',
  email: 'r@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'A',
  roles: ['reviewer'],
  permissions: ['data_security:read', 'data_security:approve'],
};
const request = {
  id: 'request-a',
  tenant_id: 'tenant-a',
  kind: 'data_access_approval',
  code: 'orders_access',
  name: '订单分析申请',
  description: null,
  parent_id: null,
  status: 'pending',
  properties: {
    object_id: 'object-orders',
    metadata_version: 3,
    purpose: '月度经营分析',
    columns: ['region', 'amount'],
    expires_at: '2026-11-01T10:00:00Z',
  },
  references: [],
  version: 2,
  created_by: 'requester',
  created_at: '2026-10-08T10:00:00Z',
  updated_at: '2026-10-08T10:00:00Z',
};
function login(permissions = user.permissions) {
  useAuthStore.setState({
    user: { ...user, permissions },
    token: 'session-reviewer',
    initialized: true,
    restoring: false,
    restoreError: null,
  });
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

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/data-access/approvals`, () =>
      HttpResponse.json({ data: [request], total: 1 }),
    ),
  );
});
afterEach(() => {
  server.resetHandlers();
  vi.restoreAllMocks();
  act(() => useAuthStore.setState({ user: null, token: null }));
});

describe('Real data access interactions', () => {
  it('keeps a stable decision receipt key after a failed request and submits only controlled fields', async () => {
    const actions = userEvent.setup();
    const submit = vi
      .spyOn(client, 'post')
      .mockRejectedValueOnce(new Error('请求超时'))
      .mockResolvedValueOnce(response({ result: { decision_id: 'decision-saved' } }));
    renderWithProviders(<DataAccessApprovalsPage />);
    await actions.click(await screen.findByRole('button', { name: '通过' }));
    await actions.type(screen.getByRole('textbox'), '同意本次月度分析');
    await actions.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(submit).toHaveBeenCalledTimes(1));
    await screen.findByText('请求超时');
    await actions.click(screen.getByRole('button', { name: '保存' }));
    await screen.findByText(/decision-saved/);
    expect(submit.mock.calls[0][0]).toBe('/product/data-access/requests/request-a/actions/approve');
    const first = submit.mock.calls[0][1];
    const second = submit.mock.calls[1][1];
    expect(second).toEqual(first);
    expect(first).toEqual({
      expected_version: 2,
      payload: { idempotency_key: expect.any(String), reason: '同意本次月度分析' },
    });
  });

  it('disables approval of the reviewers own application', async () => {
    server.use(
      http.get(`${API}/product/data-access/approvals`, () =>
        HttpResponse.json({ data: [{ ...request, created_by: user.id }], total: 1 }),
      ),
    );
    renderWithProviders(<DataAccessApprovalsPage />);
    expect(await screen.findByRole('button', { name: '通过' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '拒绝' })).toBeDisabled();
    expect(screen.getByText('不能审批自己的申请')).toBeInTheDocument();
  });

  it('removes cached approval contents and open dialogs when approval permission is withdrawn', async () => {
    const actions = userEvent.setup();
    renderWithProviders(<DataAccessApprovalsPage />);
    await actions.click(await screen.findByRole('button', { name: '通过' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(() => login(['data_security:read']));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByText('月度经营分析')).not.toBeInTheDocument();
    expect(screen.getByText('当前账号没有权限审批权限')).toBeInTheDocument();
  });

  it('shows actual effective grant fields separately from historical request state', async () => {
    const actions = userEvent.setup();
    login(['data_security:read']);
    server.use(
      http.get(`${API}/product/schemas`, () => HttpResponse.json({ data: [] })),
      http.get(`${API}/product/data-access/grants`, () =>
        HttpResponse.json({
          data: [
            {
              id: 'grant-a',
              request_id: 'request-a',
              metadata_id: 'object-orders',
              metadata_version: 3,
              table_catalog: 'warehouse',
              schema_name: 'main',
              table_name: 'orders',
              columns: ['region', 'amount'],
              expires_at: '2026-11-01T10:00:00Z',
            },
          ],
        }),
      ),
    );
    renderWithProviders(
      <MyDataAccessPage module={getProductModuleByPath('/security/my-access')!} />,
    );
    await actions.click(screen.getByRole('tab', { name: '有效授权' }));
    expect(await screen.findByText('warehouse.main.orders')).toBeInTheDocument();
    expect(screen.getByText('region, amount')).toBeInTheDocument();
    act(() => login([]));
    expect(screen.queryByText('warehouse.main.orders')).not.toBeInTheDocument();
  });

  it('renders persisted denial evidence rather than success without an event', async () => {
    server.use(
      http.get(`${API}/product/data-access/audit`, () =>
        HttpResponse.json({
          data: [
            {
              id: 'audit-denied',
              actor_id: 'requester',
              action: 'query',
              outcome: 'denied',
              evidence: { query_hash: 'a'.repeat(64), columns: ['private_column'] },
              created_at: '2026-10-08T10:00:00Z',
            },
          ],
          total: 1,
        }),
      ),
    );
    renderWithProviders(<DataAccessAuditPage />);
    expect(await screen.findByText('拒绝访问')).toBeInTheDocument();
    expect(screen.getByText('查询访问')).toBeInTheDocument();
    expect(screen.getByText(/private_column/)).toBeInTheDocument();
    expect(screen.queryByText('成功')).not.toBeInTheDocument();
  });

  it('preserves zero observed collection counts and an empty data failure ratio', () => {
    const view = renderWithProviders(
      <JobNodeResult
        result={{
          reflection_committed: true,
          collected_tables: 0,
          collected_columns: 0,
          created_objects: 0,
          updated_objects: 0,
          unchanged_objects: 0,
          missing_objects: 2,
        }}
      />,
    );
    expect(screen.getAllByText('0')).toHaveLength(5);
    expect(screen.getByText('2')).toBeInTheDocument();
    view.unmount();
    renderWithProviders(
      <JobNodeResult
        result={{
          checked_rows: 0,
          failed_rows: 0,
          failure_ratio: null,
          status: 'empty_data',
          evaluation_scope: 'full_table',
        }}
      />,
    );
    expect(screen.getByText(/失败比例/)).toHaveTextContent('—');
    expect(screen.queryByText('0.00%')).not.toBeInTheDocument();
  });
});
