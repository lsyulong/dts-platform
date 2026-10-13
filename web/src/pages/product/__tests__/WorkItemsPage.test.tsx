import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { beforeAll, afterAll, beforeEach, afterEach, describe, it, expect } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import WorkItemsPage from '../WorkItemsPage';
import { API, login, item, observation, workHandler, workResponse } from './projection-fixtures';

let cache: QueryClient;
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  cache = createTestQueryClient();
  localStorage.clear();
  login();
  workHandler({
    unread_messages: { observation: observation('unread_messages', 1), data: [item()] },
  });
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
});
function renderPage() {
  return renderWithProviders(<WorkItemsPage />, { queryClient: cache });
}
describe('Actual personal work-item projection', () => {
  it('opens recipient-owned message source for an authenticated user without system management', async () => {
    login([]);
    renderPage();
    expect(await screen.findByText('实际待办消息')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '前往来源处理' })).toHaveAttribute(
      'href',
      '/system/messages',
    );
    expect(screen.queryByRole('button', { name: '完成' })).not.toBeInTheDocument();
  });
  it('keeps a source read failure unknown instead of displaying a fabricated zero', async () => {
    workHandler({
      unread_messages: { observation: observation('unread_messages', null, 'unknown'), data: [] },
    });
    renderPage();
    const title = await screen.findByRole('heading', { name: '未读站内消息' }),
      section = title.closest('section');
    expect(section).not.toBeNull();
    if (!section) throw new Error('section missing');
    expect(within(section).getByText('数量未知')).toBeInTheDocument();
    expect(within(section).queryByText('0 条')).not.toBeInTheDocument();
  });
  it('hides previous work items immediately when a refreshed source request is denied', async () => {
    renderPage();
    await screen.findByText('实际待办消息');
    server.use(
      http.get(`${API}/product/work-items`, () =>
        HttpResponse.json({ message: 'actual access denied' }, { status: 403 }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '刷新待办' }));
    await waitFor(() => expect(screen.queryByText('实际待办消息')).not.toBeInTheDocument());
    expect(screen.queryByRole('link', { name: '前往来源处理' })).not.toBeInTheDocument();
  });
  it('does not reuse the old results while a new permission scope is loading', async () => {
    renderPage();
    await screen.findByText('实际待办消息');
    let release: () => void = () => {};
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/work-items`, async ({ request }) => {
        await pending;
        return HttpResponse.json(workResponse(request));
      }),
    );
    await act(async () => login(['development:read']));
    expect(screen.queryByText('实际待办消息')).not.toBeInTheDocument();
    release();
  });
  it('paginates one actual source without treating all source counts as one queue', async () => {
    server.use(
      http.get(`${API}/product/work-items`, ({ request }) => {
        const offset = Number(new URL(request.url).searchParams.get('offset') ?? 0);
        return HttpResponse.json(
          workResponse(request, {
            unread_messages: {
              observation: observation('unread_messages', 21),
              data: offset
                ? [item('last')]
                : Array.from({ length: 20 }, (_, i) => ({
                    ...item(`msg-${i}`),
                    title: `待办 ${i}`,
                  })),
            },
          }),
        );
      }),
    );
    renderPage();
    await screen.findByText('待办 0');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: '待办来源' }),
      'unread_messages',
    );
    await screen.findByRole('button', { name: '下一页' });
    await userEvent.click(screen.getByRole('button', { name: '下一页' }));
    expect(await screen.findByText('实际待办消息')).toBeInTheDocument();
    expect(screen.queryByText('待办 0')).not.toBeInTheDocument();
  });
  it('rejects a returned projection belonging to another tenant', async () => {
    server.use(
      http.get(`${API}/product/work-items`, ({ request }) =>
        HttpResponse.json({
          ...workResponse(request, {
            unread_messages: { observation: observation('unread_messages', 1), data: [item()] },
          }),
          tenant_id: 'other-tenant',
        }),
      ),
    );
    renderPage();
    await screen.findByRole('alert');
    expect(screen.queryByText('实际待办消息')).not.toBeInTheDocument();
  });
  it('does not show a source link when the current user lacks its explicit required permission', async () => {
    workHandler({
      queued_jobs: {
        observation: observation('queued_jobs', 1),
        data: [
          { ...item('job'), title: '实际等待运行', required_permissions: ['development:execute'] },
        ],
      },
    });
    login(['development:read']);
    renderPage();
    await screen.findByText('实际等待运行');
    expect(screen.queryByRole('link', { name: '前往来源处理' })).not.toBeInTheDocument();
  });
});
