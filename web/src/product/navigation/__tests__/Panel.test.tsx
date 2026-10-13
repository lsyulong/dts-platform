import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import NavigationPolicyPanel from '../../../pages/product/NavigationPolicyPanel';
import { API, gate, login, policy, receipt, template } from './fixtures';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/navigation/current`, () => HttpResponse.json(policy())),
    http.get(`${API}/product/navigation/history`, () => HttpResponse.json({ data: [], total: 0 })),
    http.get(`${API}/product/resources/menu_template`, () =>
      HttpResponse.json({ data: [template], total: 1 }),
    ),
    http.get(`${API}/product/resources/menu_template/${template.id}`, () =>
      HttpResponse.json(template),
    ),
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

async function openAssignment() {
  await screen.findByRole('option', { name: '财务导航模板 · v7' });
  await userEvent.selectOptions(screen.getByLabelText('真实菜单模板'), template.id);
  await waitFor(() => expect(screen.getByRole('button', { name: '分配模板' })).toBeEnabled());
  await userEvent.click(screen.getByRole('button', { name: '分配模板' }));
  return screen.getByRole('dialog');
}

describe('navigation management', () => {
  it('does not fetch templates/history or expose assignment actions with read permission alone', async () => {
    login(['system:read']);
    let administrativeReads = 0;
    server.use(
      http.get(`${API}/product/navigation/history`, () => {
        administrativeReads++;
        return HttpResponse.json({ data: [], total: 0 });
      }),
      http.get(`${API}/product/resources/menu_template`, () => {
        administrativeReads++;
        return HttpResponse.json({ data: [template], total: 1 });
      }),
    );
    renderWithProviders(<NavigationPolicyPanel />);
    await screen.findByText('真实模型目录');
    expect(screen.queryByRole('button', { name: '分配模板' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '撤销当前策略' })).not.toBeInTheDocument();
    expect(administrativeReads).toBe(0);
  });

  it('retries the same CAS assignment with the same UUID after failure and closing the dialog', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${API}/product/navigation/assign/${template.id}`, async ({ request }) => {
        bodies.push(await request.json());
        return bodies.length === 1
          ? HttpResponse.json({ message: 'temporary failure' }, { status: 503 })
          : HttpResponse.json(receipt(true));
      }),
    );
    renderWithProviders(<NavigationPolicyPanel />);
    let dialog = await openAssignment();
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await within(dialog).findByText('temporary failure');
    await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    dialog = await openAssignment();
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await screen.findByText(/已取得原请求的重试回执/);
    expect(bodies).toHaveLength(2);
    expect(bodies[0]).toEqual(bodies[1]);
    expect(bodies[0]).toEqual({
      expected_template_version: 7,
      expected_assignment_version: 4,
      expires_at: null,
      idempotency_key: expect.stringMatching(UUID),
    });
  });

  it('does not reopen a closed pending dialog or show a late receipt, while refreshing actual policy', async () => {
    const pending = gate();
    let posted = false;
    let committed = false;
    server.use(
      http.get(`${API}/product/navigation/current`, () =>
        HttpResponse.json(policy({ assignment_version: committed ? 5 : 4 })),
      ),
      http.post(`${API}/product/navigation/assign/${template.id}`, async () => {
        posted = true;
        await pending.promise;
        committed = true;
        return HttpResponse.json(receipt());
      }),
    );
    renderWithProviders(<NavigationPolicyPanel />);
    const dialog = await openAssignment();
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await waitFor(() => expect(posted).toBe(true));
    await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    await act(async () => {
      pending.release();
      await pending.promise;
    });
    await screen.findByText('已分配模板 · 策略版本 5');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByText(/导航策略操作已提交/)).not.toBeInTheDocument();
  });

  it('discards a pending administrator result after identity and permission change', async () => {
    const pending = gate();
    let posted = false;
    server.use(
      http.post(`${API}/product/navigation/assign/${template.id}`, async () => {
        posted = true;
        await pending.promise;
        return HttpResponse.json(receipt());
      }),
    );
    renderWithProviders(<NavigationPolicyPanel />);
    const dialog = await openAssignment();
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await waitFor(() => expect(posted).toBe(true));
    act(() => login(['system:read'], 'other-session', 'other-user'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await act(async () => {
      pending.release();
      await pending.promise;
    });
    await screen.findByText('真实模型目录');
    expect(screen.queryByText(/receipt-original-a/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '分配模板' })).not.toBeInTheDocument();
  });

  it('sends the actual revocation reason, assignment identity and CAS version with a stable retry key', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${API}/product/navigation/revoke`, async ({ request }) => {
        bodies.push(await request.json());
        return bodies.length === 1
          ? HttpResponse.json({ message: 'retry revoke' }, { status: 503 })
          : HttpResponse.json(receipt(true, 'revoked'));
      }),
    );
    renderWithProviders(<NavigationPolicyPanel />);
    await waitFor(() => expect(screen.getByRole('button', { name: '撤销当前策略' })).toBeEnabled());
    await userEvent.click(screen.getByRole('button', { name: '撤销当前策略' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.type(within(dialog).getByLabelText('撤销原因'), '组织导航调整');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await within(dialog).findByText('retry revoke');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await screen.findByText(/已取得原请求的重试回执/);
    expect(bodies[0]).toEqual(bodies[1]);
    expect(bodies[0]).toEqual({
      assignment_id: 'assignment-a',
      expected_assignment_version: 4,
      reason: '组织导航调整',
      idempotency_key: expect.stringMatching(UUID),
    });
  });
});
