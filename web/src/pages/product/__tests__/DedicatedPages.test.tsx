import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';
import { AxiosHeaders, type AxiosResponse } from 'axios';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import client from '../../../api/client';
import { queryClient } from '../../../lib/queryClient';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import ProfilePage from '../ProfilePage';
import TaskLogsPage from '../TaskLogsPage';

const API = 'http://localhost:8080/api/v1';
const userA = {
  id: 'self-a',
  username: 'self-user',
  display_name: 'User A',
  email: 'a@example.invalid',
  tenant_id: 'tenant-a',
  tenant_name: 'Tenant A',
  roles: ['viewer'],
  permissions: [],
};
const userB = {
  ...userA,
  id: 'self-b',
  username: 'other-user',
  display_name: 'User B',
  email: 'b@example.invalid',
  tenant_id: 'tenant-b',
  tenant_name: 'Tenant B',
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

function response<T>(data: T, status = 200): AxiosResponse<T> {
  return {
    data,
    status,
    statusText: 'OK',
    headers: new AxiosHeaders(),
    config: { headers: new AxiosHeaders() },
  };
}

function PathProbe() {
  return <p data-testid="current-path">{useLocation().pathname}</p>;
}

async function enterPassword(interactions: ReturnType<typeof userEvent.setup>) {
  await interactions.click(screen.getByRole('tab', { name: '账户安全' }));
  await interactions.type(screen.getByLabelText('当前密码'), 'current-secret');
  await interactions.type(screen.getByLabelText('新密码'), 'new-safe-secret-123');
  await interactions.type(screen.getByLabelText('确认新密码'), 'new-safe-secret-123');
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  queryClient.clear();
  localStorage.clear();
  login();
});
afterEach(() => {
  server.resetHandlers();
  queryClient.clear();
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('Personal account business pages', () => {
  it('updates the real current account without requiring user management permission', async () => {
    let authorization: string | null = null;
    server.use(
      http.patch(`${API}/auth/profile`, async ({ request }) => {
        authorization = request.headers.get('Authorization');
        const body = await request.json();
        expect(body).toEqual({ email: 'changed@example.invalid', display_name: 'User A' });
        return HttpResponse.json({ ...userA, email: 'changed@example.invalid' });
      }),
    );
    renderWithProviders(<ProfilePage />);
    const interactions = userEvent.setup();
    await interactions.clear(screen.getByLabelText('电子邮箱'));
    await interactions.type(screen.getByLabelText('电子邮箱'), 'changed@example.invalid');
    await interactions.click(screen.getByRole('button', { name: '保存基本资料' }));
    expect(await screen.findByText('基本资料已保存')).toBeInTheDocument();
    expect(authorization).toBe('Bearer session-a');
    expect(useAuthStore.getState().user?.email).toBe('changed@example.invalid');
    expect(JSON.parse(localStorage.getItem('user') || 'null').email).toBe(
      'changed@example.invalid',
    );
  });

  it('rejects account responses from a different tenant', async () => {
    server.use(http.patch(`${API}/auth/profile`, () => HttpResponse.json(userB)));
    renderWithProviders(<ProfilePage />);
    const interactions = userEvent.setup();
    await interactions.clear(screen.getByLabelText('显示名称'));
    await interactions.type(screen.getByLabelText('显示名称'), 'Changed A');
    await interactions.click(screen.getByRole('button', { name: '保存基本资料' }));
    expect(await screen.findByText('服务器返回的账户信息不正确')).toBeInTheDocument();
    expect(useAuthStore.getState().user?.id).toBe(userA.id);
    expect(JSON.parse(localStorage.getItem('user') || 'null').id).toBe(userA.id);
  });

  it('does not let a late profile response replace a newer account session', async () => {
    const pending = deferred<AxiosResponse<typeof userA>>();
    const patch = vi.spyOn(client, 'patch').mockImplementationOnce(() => pending.promise);
    renderWithProviders(<ProfilePage />);
    const interactions = userEvent.setup();
    await interactions.clear(screen.getByLabelText('显示名称'));
    await interactions.type(screen.getByLabelText('显示名称'), 'Changed A');
    await interactions.click(screen.getByRole('button', { name: '保存基本资料' }));
    expect(patch).toHaveBeenCalledOnce();
    await act(async () => {
      login(userB, 'session-b');
      pending.resolve(response({ ...userA, display_name: 'Changed A' }));
      await pending.promise;
    });
    expect(useAuthStore.getState().user?.id).toBe(userB.id);
    expect(JSON.parse(localStorage.getItem('user') || 'null').id).toBe(userB.id);
    expect(screen.queryByText('基本资料已保存')).not.toBeInTheDocument();
  });

  it('preserves the current session when current-password validation fails', async () => {
    server.use(
      http.post(`${API}/auth/change-password`, () =>
        HttpResponse.json({ message: '当前密码不正确' }, { status: 400 }),
      ),
    );
    renderWithProviders(<ProfilePage />);
    const interactions = userEvent.setup();
    await enterPassword(interactions);
    await interactions.click(screen.getByRole('button', { name: '修改密码并重新登录' }));
    expect(await screen.findByText('当前密码不正确')).toBeInTheDocument();
    expect(localStorage.getItem('token')).toBe('session-a');
    expect(useAuthStore.getState().user?.id).toBe(userA.id);
    expect(JSON.stringify(localStorage)).not.toContain('current-secret');
    expect(JSON.stringify(localStorage)).not.toContain('new-safe-secret-123');
  });

  it('clears its own session and navigates to sign-in only after confirmed password change', async () => {
    server.use(
      http.post(`${API}/auth/change-password`, async ({ request }) => {
        expect(request.headers.get('Authorization')).toBe('Bearer session-a');
        expect(await request.json()).toEqual({
          current_password: 'current-secret',
          new_password: 'new-safe-secret-123',
        });
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderWithProviders(
      <>
        <ProfilePage />
        <PathProbe />
      </>,
    );
    const interactions = userEvent.setup();
    await enterPassword(interactions);
    await interactions.click(screen.getByRole('button', { name: '修改密码并重新登录' }));
    await waitFor(() => expect(screen.getByTestId('current-path')).toHaveTextContent('/login'));
    expect(localStorage.getItem('token')).toBeNull();
    expect(localStorage.getItem('user')).toBeNull();
    expect(useAuthStore.getState().user).toBeNull();
  });

  it('preserves a newer user session when the previous user password change finishes', async () => {
    const pending = deferred<AxiosResponse<null>>();
    const post = vi.spyOn(client, 'post').mockImplementationOnce(() => pending.promise);
    renderWithProviders(
      <>
        <ProfilePage />
        <PathProbe />
      </>,
    );
    const interactions = userEvent.setup();
    await enterPassword(interactions);
    await interactions.click(screen.getByRole('button', { name: '修改密码并重新登录' }));
    expect(post).toHaveBeenCalledOnce();
    await act(async () => {
      login(userB, 'session-b');
      pending.resolve(response(null, 204));
      await pending.promise;
    });
    expect(localStorage.getItem('token')).toBe('session-b');
    expect(useAuthStore.getState().user?.id).toBe(userB.id);
    expect(screen.getByTestId('current-path')).toHaveTextContent('/');
  });
});

describe('Persisted run logs page', () => {
  it('waits for real task and run selection, paginates server logs, and filters only the loaded page', async () => {
    const offsets: string[] = [];
    server.use(
      http.get(`${API}/tasks/:taskId/runs/:runId/logs`, ({ request, params }) => {
        const url = new URL(request.url);
        offsets.push(url.searchParams.get('offset') || '');
        expect(params.taskId).toBe('task-1');
        expect(params.runId).toBe('run-1');
        expect(url.searchParams.get('limit')).toBe('200');
        expect(request.headers.get('Authorization')).toBe('Bearer session-a');
        const entries =
          url.searchParams.get('offset') === '200'
            ? [
                {
                  id: 'last',
                  sequence: 200,
                  timestamp: '2026-10-08T12:00:00Z',
                  level: 'WARN',
                  component: 'engine',
                  message: 'Final stored warning',
                },
              ]
            : Array.from({ length: 200 }, (_, index) => ({
                id: `log-${index}`,
                sequence: index,
                timestamp: '2026-10-08T12:00:00Z',
                level: index === 0 ? 'DEBUG' : 'INFO',
                component: 'engine',
                message: index === 0 ? 'Stored debug detail' : `Stored page entry ${index}`,
              }));
        return HttpResponse.json({ data: entries, total: 201 });
      }),
    );
    renderWithProviders(<TaskLogsPage />);
    const interactions = userEvent.setup();
    await screen.findByRole('option', { name: 'MySQL to PostgreSQL Sync' });
    expect(offsets).toEqual([]);
    await interactions.selectOptions(screen.getByLabelText('选择任务'), 'task-1');
    await screen.findByRole('option', { name: /run-1/ });
    expect(offsets).toEqual([]);
    await interactions.selectOptions(screen.getByLabelText('选择运行实例'), 'run-1');
    expect(await screen.findByText('Stored debug detail')).toBeInTheDocument();
    await interactions.selectOptions(screen.getByLabelText('当前页日志级别'), 'DEBUG');
    expect(screen.getByText('Stored debug detail')).toBeInTheDocument();
    expect(screen.queryByText('Stored page entry 1')).not.toBeInTheDocument();
    await interactions.click(screen.getByRole('button', { name: '下一页' }));
    expect(await screen.findByText('当前页没有此级别日志')).toBeInTheDocument();
    await interactions.selectOptions(screen.getByLabelText('当前页日志级别'), 'WARN');
    expect(await screen.findByText('Final stored warning')).toBeInTheDocument();
    expect(offsets).toEqual(['0', '200']);
  });

  it('shows the actual logs endpoint failure without replacing it with the run error message', async () => {
    server.use(
      http.get(`${API}/tasks/:taskId/runs/:runId/logs`, () =>
        HttpResponse.json({ message: '日志未保留，无法读取' }, { status: 500 }),
      ),
    );
    renderWithProviders(<TaskLogsPage />);
    const interactions = userEvent.setup();
    await screen.findByRole('option', { name: 'MySQL to PostgreSQL Sync' });
    await interactions.selectOptions(screen.getByLabelText('选择任务'), 'task-1');
    await screen.findByRole('option', { name: /run-3/ });
    await interactions.selectOptions(screen.getByLabelText('选择运行实例'), 'run-3');
    expect(await screen.findByText('日志未保留，无法读取')).toBeInTheDocument();
    expect(screen.queryByText(/Connection timeout/)).not.toBeInTheDocument();
  });
});
