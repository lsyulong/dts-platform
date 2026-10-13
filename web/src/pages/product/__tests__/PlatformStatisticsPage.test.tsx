import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { beforeAll, afterAll, beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import PlatformStatisticsPage from '../PlatformStatisticsPage';
import {
  API,
  login,
  observation,
  statisticsHandler,
  statisticsResponse,
} from './projection-fixtures';

let cache: QueryClient;
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  cache = createTestQueryClient();
  localStorage.clear();
  login();
  statisticsHandler({
    sync_definitions: {
      observation: observation('sync_definitions', 9),
      data: [{ dimension: 'draft', count: 9 }],
    },
    sync_runs: {
      observation: observation('sync_runs', 2),
      data: [{ dimension: 'failed', count: 2 }],
    },
  });
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
function renderPage(view: 'system' | 'usage' = 'system') {
  return renderWithProviders(<PlatformStatisticsPage view={view} />, { queryClient: cache });
}
function section(title: HTMLElement) {
  const value = title.closest('section');
  if (!value) throw new Error('section missing');
  return within(value);
}
describe('Persisted statistics and observed scope', () => {
  it('keeps definitions distinct from actual runs and displays source evidence', async () => {
    renderPage();
    const definitions = await screen.findByRole('heading', { name: '同步任务定义' }),
      runs = screen.getByRole('heading', { name: '同步任务运行' });
    expect(section(definitions).getAllByText('9')).toHaveLength(2);
    expect(section(runs).getAllByText('2')).toHaveLength(2);
    expect(screen.getAllByText('tenant=current; actor=current; persisted rows')).not.toHaveLength(
      0,
    );
  });
  it('does not replace a read failure with zero even alongside known empty sources', async () => {
    statisticsHandler({
      sync_runs: { observation: observation('sync_runs', null, 'unknown'), data: [] },
    });
    renderPage();
    const title = await screen.findByRole('heading', { name: '同步任务运行' });
    expect(section(title).getByText('未知')).toBeInTheDocument();
    expect(section(title).queryByText('0')).not.toBeInTheDocument();
  });
  it('hides cached counts when the server denies the refreshed statistics', async () => {
    renderPage();
    await screen.findByRole('heading', { name: '同步任务定义' });
    server.use(
      http.get(`${API}/product/statistics`, () =>
        HttpResponse.json({ message: 'permission changed' }, { status: 403 }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '刷新统计' }));
    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: '同步任务定义' })).not.toBeInTheDocument(),
    );
  });
  it('makes no system-statistics request for a user without system read permission', async () => {
    let requests = 0;
    server.use(
      http.get(`${API}/product/statistics`, () => {
        requests++;
        return HttpResponse.json({});
      }),
    );
    login([]);
    renderPage();
    expect(await screen.findByText('当前无权读取系统统计。')).toBeInTheDocument();
    expect(requests).toBe(0);
  });
  it('allows personal usage without system read and does not count pending service reservations as success', async () => {
    login(['services:read']);
    statisticsHandler({
      service_calls: {
        observation: observation('service_calls', 3),
        data: [
          { dimension: 'pending', count: 2 },
          { dimension: 'failed', count: 1 },
        ],
      },
    });
    renderPage('usage');
    const title = await screen.findByRole('heading', { name: '我负责的服务调用' });
    expect(section(title).getByText('已预留待完成')).toBeInTheDocument();
    expect(section(title).getByText('失败')).toBeInTheDocument();
    expect(section(title).queryByText('成功')).not.toBeInTheDocument();
  });
  it('drops the previous scope while replacement permission data is pending', async () => {
    renderPage();
    await screen.findByRole('heading', { name: '同步任务定义' });
    let release: () => void = () => {};
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`${API}/product/statistics`, async ({ request }) => {
        await pending;
        return HttpResponse.json(statisticsResponse(request));
      }),
    );
    await act(async () => login(['system:read']));
    expect(screen.queryByRole('heading', { name: '同步任务定义' })).not.toBeInTheDocument();
    release();
  });
  it('rejects an invalid time window before sending another query', async () => {
    let requests = 0;
    server.use(
      http.get(`${API}/product/statistics`, ({ request }) => {
        requests++;
        return HttpResponse.json(statisticsResponse(request));
      }),
    );
    renderPage();
    await screen.findByRole('heading', { name: '账户记录' });
    const start = screen.getByLabelText('开始时间（UTC，包含）'),
      end = screen.getByLabelText('结束时间（UTC，不包含）');
    await userEvent.clear(start);
    await userEvent.type(start, '2026-10-10T00:00');
    await userEvent.clear(end);
    await userEvent.type(end, '2026-10-09T00:00');
    await userEvent.click(screen.getByRole('button', { name: '应用时间窗口' }));
    await screen.findByText('请选择正数且最多366天的UTC时间窗口，结束时间不能在未来。');
    expect(requests).toBe(1);
  });
  it('will not export stale cached counts after the fresh export read is denied', async () => {
    const create = vi.fn(() => 'blob:test');
    vi.stubGlobal(
      'URL',
      class extends URL {
        static createObjectURL = create;
        static revokeObjectURL = vi.fn();
      },
    );
    renderPage();
    await screen.findByRole('heading', { name: '同步任务定义' });
    server.use(
      http.get(`${API}/product/statistics`, () =>
        HttpResponse.json({ message: 'denied' }, { status: 403 }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '导出实际JSON统计' }));
    await screen.findByText('导出前读取失败，未导出旧缓存统计。');
    expect(create).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });
});
