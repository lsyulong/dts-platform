import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { useAuthStore } from '../../../stores/auth';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import SensitiveFindingsPage from '../SensitiveFindingsPage';
import type { SensitiveFinding } from '../sensitive-api';

const API = 'http://localhost:8080/api/v1';
const user = {
  id: 'reader',
  tenant_id: 'tenant',
  tenant_name: 'Tenant',
  username: 'reader',
  email: 'reader@example.invalid',
  roles: ['sensitive_reader'],
  permissions: ['sensitive_data:read', 'sensitive_data:write', 'data_security:all_data'],
};
const finding: SensitiveFinding = {
  id: 'actual-finding',
  tenant_id: 'tenant',
  task_id: 'task',
  task_version: 2,
  job_id: 'actual-job',
  node_id: 'recognition',
  object_id: 'metadata',
  object_version: 1,
  rule_id: 'rule',
  rule_version: 2,
  classification_id: 'class',
  classification_version: 2,
  column_name: 'customer_email',
  evidence: {
    column_name: 'customer_email',
    data_type: 'VARCHAR',
    checked_values: 2,
    matched_values: 1,
    null_values: 1,
  },
  evaluation_scope: 'sample',
  sampled_rows: 3,
  definition_hash: 'a'.repeat(64),
  status: 'pending',
  version: 1,
  reviewed_by: null,
  review_reason: null,
  created_at: '2026-10-09T10:00:00Z',
  reviewed_at: null,
};
let cache: QueryClient;
function login(permissions = user.permissions, token = 'reader-session') {
  const account = { ...user, permissions };
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
function response(value = finding) {
  server.use(
    http.get(`${API}/product/sensitive-findings`, () =>
      HttpResponse.json({ data: [value], total: 1 }),
    ),
    http.get(`${API}/product/sensitive-findings/:id`, () => HttpResponse.json(value)),
  );
}
function renderPage(route = '/security/discovery') {
  return renderWithProviders(<SensitiveFindingsPage />, { route, queryClient: cache });
}
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  cache = createTestQueryClient();
  localStorage.clear();
  login();
  response();
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
});

describe('Actual sensitive evidence and CAS review', () => {
  it('shows observed sample counts and actual source evidence without raw samples', async () => {
    renderPage('/security/discovery?finding=actual-finding');
    expect(await screen.findByText('actual-job / recognition')).toBeInTheDocument();
    expect(screen.getByText('3 实际行')).toBeInTheDocument();
    expect(screen.getAllByText('1 / 2 / 1')).toHaveLength(2);
    expect(screen.getByRole('link', { name: '查看实际运行来源' })).toHaveAttribute(
      'href',
      '/security/recognition-tasks?job=actual-job',
    );
    expect(screen.getByText(/抽样未匹配不代表整表安全/)).toBeInTheDocument();
  });
  it('hides cached list and selected evidence immediately after a denied refresh', async () => {
    renderPage('/security/discovery?finding=actual-finding');
    expect(await screen.findByText('actual-job / recognition')).toBeInTheDocument();
    server.use(
      http.get(`${API}/product/sensitive-findings`, () =>
        HttpResponse.json({ message: 'Current evidence grant revoked' }, { status: 403 }),
      ),
    );
    await act(async () => {
      await cache.refetchQueries({ queryKey: ['sensitive-findings'] });
    });
    expect(await screen.findByText('Current evidence grant revoked')).toBeInTheDocument();
    expect(screen.queryByText('1 / 2 / 1')).not.toBeInTheDocument();
    expect(screen.queryByText('actual-job / recognition')).not.toBeInTheDocument();
    expect(
      cache
        .getQueriesData({ queryKey: ['sensitive-findings'] })
        .some(([, value]) => value !== undefined),
    ).toBe(true);
  });
  it('also hides cached list counts when the selected current evidence check is denied', async () => {
    renderPage('/security/discovery?finding=actual-finding');
    await screen.findByText('actual-job / recognition');
    server.use(
      http.get(`${API}/product/sensitive-findings/:id`, () =>
        HttpResponse.json({ message: 'Selected evidence authorization lost' }, { status: 403 }),
      ),
    );
    await act(async () => {
      await cache.refetchQueries({ queryKey: ['sensitive-finding'] });
    });
    await screen.findByText('Selected evidence authorization lost');
    expect(screen.queryByText('1 / 2 / 1')).not.toBeInTheDocument();
    expect(screen.queryByText('actual-job / recognition')).not.toBeInTheDocument();
  });
  it('requires a reason and sends the actual finding version; a conflict does not confirm', async () => {
    let received: unknown;
    server.use(
      http.post(`${API}/product/sensitive-findings/:id/confirm`, async ({ request, params }) => {
        expect(params.id).toBe(finding.id);
        expect(request.headers.get('Authorization')).toBe('Bearer reader-session');
        received = await request.json();
        return HttpResponse.json({ message: 'Finding version conflict' }, { status: 409 });
      }),
    );
    renderPage();
    await screen.findByText('customer_email');
    const action = userEvent.setup();
    await action.click(screen.getByRole('button', { name: '确认建议' }));
    expect(screen.getByRole('button', { name: '提交复核' })).toBeDisabled();
    await action.type(
      screen.getByRole('textbox', { name: '复核意见（必填）' }),
      '确认实际字段用途',
    );
    await action.click(screen.getByRole('button', { name: '提交复核' }));
    expect(received).toEqual({ expected_version: 1, reason: '确认实际字段用途' });
    expect(await screen.findByText('Finding version conflict')).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    await action.click(screen.getByRole('button', { name: '取消' }));
    expect(screen.getByRole('button', { name: '确认建议' })).toBeInTheDocument();
  });
  it('accepts a real review receipt and refreshes state without inventing classification enforcement', async () => {
    let current = finding;
    server.use(
      http.get(`${API}/product/sensitive-findings`, () =>
        HttpResponse.json({ data: [current], total: 1 }),
      ),
      http.post(`${API}/product/sensitive-findings/:id/dismiss`, async ({ request }) => {
        const body = await request.json();
        expect(body).toEqual({ expected_version: 1, reason: '确认不是当前业务敏感字段' });
        current = {
          ...finding,
          status: 'dismissed',
          version: 2,
          reviewed_by: user.id,
          review_reason: '确认不是当前业务敏感字段',
          reviewed_at: '2026-10-09T10:05:00Z',
        };
        return HttpResponse.json({
          id: finding.id,
          status: 'dismissed',
          version: 2,
          reviewed_by: user.id,
          review_reason: current.review_reason,
          reviewed_at: current.reviewed_at,
          classification_enforced: false,
        });
      }),
    );
    renderPage();
    await screen.findByText('customer_email');
    const action = userEvent.setup();
    await action.click(screen.getByRole('button', { name: '忽略发现' }));
    await action.type(
      screen.getByRole('textbox', { name: '复核意见（必填）' }),
      '确认不是当前业务敏感字段',
    );
    await action.click(screen.getByRole('button', { name: '提交复核' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: '确认建议' })).not.toBeInTheDocument(),
    );
  });
  it('removes old counts while a new permission scope is awaiting authorization', async () => {
    renderPage();
    expect(await screen.findByText('1 / 2 / 1')).toBeInTheDocument();
    let release: () => void = () => {};
    const pause = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/sensitive-findings`, async () => {
        await pause;
        return HttpResponse.json({ message: 'No current data grant' }, { status: 403 });
      }),
    );
    act(() => login(['sensitive_data:read']));
    expect(screen.queryByText('1 / 2 / 1')).not.toBeInTheDocument();
    release();
    expect(await screen.findByText('No current data grant')).toBeInTheDocument();
  });
  it('does not offer review controls to a reader without write permission', async () => {
    login(['sensitive_data:read']);
    renderPage();
    await screen.findByText('customer_email');
    expect(screen.queryByRole('button', { name: '确认建议' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '忽略发现' })).not.toBeInTheDocument();
  });
});
