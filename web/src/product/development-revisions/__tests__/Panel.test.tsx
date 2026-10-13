import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import DevelopmentRevisionPanel from '../../../pages/product/DevelopmentRevisionPanel';
import { API, diff, gate, login, revision, source, summary } from './fixtures';
const path = `${API}/product/development/sql_document/document`;
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/resources/sql_document/document`, () => HttpResponse.json(source())),
    http.get(`${path}/revisions`, () => HttpResponse.json({ data: [summary()], total: 1 })),
    http.get(`${path}/revisions/:revision`, ({ params }) =>
      HttpResponse.json(revision(Number(String(params.revision).split('-')[1]))),
    ),
    http.get(`${path}/diff`, () => HttpResponse.json(diff())),
  );
});
afterEach(() => {
  cleanup();
  server.resetHandlers();
  act(() =>
    useAuthStore.setState({
      user: null,
      token: null,
      initialized: true,
      restoring: false,
      restoreError: null,
    }),
  );
  localStorage.clear();
});
const renderPanel = () =>
  renderWithProviders(<DevelopmentRevisionPanel kind="sql_document" resourceId="document" />);
async function openRestore() {
  await userEvent.click(await screen.findByRole('button', { name: '查看修订 v3' }));
  await screen.findByText(/不可变修订详情/);
  await userEvent.click(screen.getByRole('button', { name: '恢复此修订' }));
  const dialog = screen.getByRole('dialog');
  await userEvent.type(within(dialog).getByLabelText('修订原因'), '恢复经过核对的定义');
  return dialog;
}
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
describe('actual development revision interactions', () => {
  it('read permission allows immutable detail and real diff without save or restore', async () => {
    login(['development:read']);
    renderPanel();
    await screen.findByText(/Actual SQL Document/);
    expect(screen.getByRole('button', { name: '保存当前修订' })).toBeDisabled();
    await userEvent.click(await screen.findByRole('button', { name: '查看修订 v3' }));
    await screen.findByText(/不可变修订详情/);
    expect(screen.getByRole('button', { name: '恢复此修订' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: '查看差异' }));
    await screen.findByText('/properties/sql');
    expect(screen.getByText(/可编辑定义对比/)).toHaveTextContent('v3 → v5');
    expect(screen.getAllByText(/SELECT 1 AS value/).length).toBeGreaterThan(0);
  });
  it('save sends current actual version and same stable UUID after failure and close/reopen', async () => {
    const requests: Record<string, unknown>[] = [];
    server.use(
      http.post(`${path}/revisions`, async ({ request }) => {
        requests.push(await request.json());
        if (requests.length === 1)
          return HttpResponse.json({ message: 'save unavailable' }, { status: 503 });
        return HttpResponse.json({
          revision: summary(5),
          receipt_id: 'save-real-receipt',
          created: true,
          replayed: false,
        });
      }),
    );
    renderPanel();
    await screen.findByText(/Actual SQL Document/);
    await userEvent.click(screen.getByRole('button', { name: '保存当前修订' }));
    let dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('修订原因'), '保存核对后的定义');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await within(dialog).findByText(/save unavailable/);
    await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    await userEvent.click(screen.getByRole('button', { name: '保存当前修订' }));
    dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('修订原因'), '保存核对后的定义');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await screen.findByText(/save-real-receipt/);
    expect(requests[0]).toEqual({
      expected_version: 5,
      reason: '保存核对后的定义',
      idempotency_key: expect.stringMatching(UUID),
    });
    expect(requests[1]).toEqual(requests[0]);
    expect(screen.getByText(/保存修订仅归档定义/)).toBeInTheDocument();
  });
  it('restore conflict preserves frozen current CAS and original UUID instead of silently overwriting', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(`${path}/revisions/revision-3/restore`, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({ message: 'version conflict' }, { status: 409 });
      }),
    );
    const queryClient = createTestQueryClient();
    renderWithProviders(<DevelopmentRevisionPanel kind="sql_document" resourceId="document" />, {
      queryClient,
    });
    const dialog = await openRestore();
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await within(dialog).findByText(/version conflict/);
    server.use(
      http.get(`${API}/product/resources/sql_document/document`, () =>
        HttpResponse.json(source(9)),
      ),
    );
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: ['development-revisions'] });
    });
    await screen.findByText(/Actual SQL Document · v9/);
    expect(dialog).toHaveTextContent('当前校验版本: v5');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await waitFor(() => expect(bodies).toHaveLength(2));
    expect(bodies[0]).toEqual(bodies[1]);
    expect(bodies[0]?.expected_version).toBe(5);
  });
  it('published source cannot restore and successful inactive restore reports a new draft', async () => {
    server.use(
      http.get(`${API}/product/resources/sql_document/document`, () =>
        HttpResponse.json(source(5, 'published')),
      ),
    );
    renderPanel();
    await userEvent.click(await screen.findByRole('button', { name: '查看修订 v3' }));
    await screen.findByText(/不可变修订详情/);
    expect(screen.getByRole('button', { name: '恢复此修订' })).toBeDisabled();
    server.use(
      http.get(`${API}/product/resources/sql_document/document`, () =>
        HttpResponse.json(source(6, 'inactive')),
      ),
      http.post(`${path}/revisions/revision-3/restore`, () =>
        HttpResponse.json({
          resource: source(7, 'draft'),
          revision: summary(),
          receipt_id: 'restore-actual-receipt',
          replayed: false,
          requires_publish: true,
        }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '刷新修订' }));
    await screen.findByText(/Actual SQL Document · v6/);
    await userEvent.click(screen.getByRole('button', { name: '恢复此修订' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('修订原因'), '已撤回后恢复');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await screen.findByText(/恢复为新草稿 v7/);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('late save receipt is ignored after authenticated identity changes', async () => {
    const delayed = gate();
    server.use(
      http.post(`${path}/revisions`, async () => {
        await delayed.promise;
        return HttpResponse.json({
          revision: summary(5),
          receipt_id: 'old-actor-receipt',
          created: true,
          replayed: false,
        });
      }),
    );
    renderPanel();
    await screen.findByText(/Actual SQL Document/);
    await userEvent.click(screen.getByRole('button', { name: '保存当前修订' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('修订原因'), 'save');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    act(() => login(['development:read'], 'new-token', 'other-actor'));
    delayed.release();
    await screen.findByText(/Actual SQL Document/);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByText(/old-actor-receipt/)).not.toBeInTheDocument();
  });
  it('history uses actual pagination and failed refresh hides cached rows and counts', async () => {
    const offsets: string[] = [];
    server.use(
      http.get(`${path}/revisions`, ({ request }) => {
        const offset = new URL(request.url).searchParams.get('offset') ?? '';
        offsets.push(offset);
        return HttpResponse.json({ data: [summary(offset === '20' ? 1 : 3)], total: 21 });
      }),
    );
    renderPanel();
    await screen.findByRole('button', { name: '查看修订 v3' });
    await userEvent.click(screen.getByRole('button', { name: '下一页' }));
    await screen.findByRole('button', { name: '查看修订 v1' });
    expect(offsets).toContain('20');
    server.use(
      http.get(`${path}/revisions`, () =>
        HttpResponse.json({ message: 'history unavailable' }, { status: 503 }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '刷新修订' }));
    await screen.findByText('修订历史读取失败，请重试');
    expect(screen.queryByRole('button', { name: '查看修订 v1' })).not.toBeInTheDocument();
    expect(screen.queryByText('21 条真实修订')).not.toBeInTheDocument();
  });
  it('closing a pending save never reopens its dialog or exposes a late receipt', async () => {
    const delayed = gate();
    let sourceReads = 0;
    server.use(
      http.get(`${API}/product/resources/sql_document/document`, () => {
        sourceReads++;
        return HttpResponse.json(source());
      }),
      http.post(`${path}/revisions`, async () => {
        await delayed.promise;
        return HttpResponse.json({
          revision: summary(5),
          receipt_id: 'closed-dialog-receipt',
          created: true,
          replayed: false,
        });
      }),
    );
    renderPanel();
    await screen.findByText(/Actual SQL Document/);
    await userEvent.click(screen.getByRole('button', { name: '保存当前修订' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('修订原因'), '核对定义');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
    await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    delayed.release();
    await waitFor(() => expect(sourceReads).toBeGreaterThan(1));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByText(/closed-dialog-receipt/)).not.toBeInTheDocument();
  });
  it('a revision from another tenant fails closed without rendering its SQL or restore action', async () => {
    server.use(
      http.get(`${path}/revisions/revision-3`, () =>
        HttpResponse.json({
          ...revision(),
          tenant_id: 'foreign-tenant',
          snapshot: {
            ...source(3),
            tenant_id: 'foreign-tenant',
            properties: { sql: 'SELECT forbidden_foreign_content' },
          },
        }),
      ),
    );
    renderPanel();
    await userEvent.click(await screen.findByRole('button', { name: '查看修订 v3' }));
    await screen.findByText('修订详情读取失败');
    expect(screen.queryByText(/forbidden_foreign_content/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '恢复此修订' })).not.toBeInTheDocument();
  });
});
