import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import OrganizationMembersPanel from '../../../pages/product/OrganizationMembersPanel';
import { API, details, employee, gate, login, member, organization, receipt } from './fixtures';

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/organizations/:id`, ({ params }) =>
      HttpResponse.json(details(String(params.id))),
    ),
    http.get(`${API}/product/organizations/:id/members`, ({ params }) =>
      HttpResponse.json({
        data: params.id === 'a' ? [member(), member('two')] : [],
        total: params.id === 'a' ? 2 : 0,
      }),
    ),
    http.get(`${API}/product/organization-members/history`, () =>
      HttpResponse.json({ data: [], total: 0 }),
    ),
    http.get(`${API}/product/resources/organization`, () =>
      HttpResponse.json({ data: [organization('a'), organization('b')], total: 2 }),
    ),
    http.get(`${API}/product/reference/users`, () =>
      HttpResponse.json({
        data: [{ id: 'free', tenant_id: 'tenant-org', username: 'FreeUser', display_name: null }],
        total: 1,
      }),
    ),
    http.get(`${API}/product/organization-members/users/:id`, ({ params }) =>
      HttpResponse.json({
        user: employee(String(params.id)),
        membership: params.id === 'free' ? null : member(String(params.id)).membership,
      }),
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
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const row = async (name: string) => {
  await screen.findByRole('cell', { name: new RegExp(name) });
  return screen.getByRole('cell', { name: new RegExp(name) }).closest('tr');
};
async function openTransfer() {
  const owner = await row('OwnerOne');
  if (!owner) throw new Error('owner row missing');
  await userEvent.click(within(owner).getByRole('button', { name: '调动' }));
  const dialog = screen.getByRole('dialog');
  await within(dialog).findByRole('option', { name: 'Org B' });
  await userEvent.selectOptions(within(dialog).getByLabelText('调动目标组织'), 'b');
  await waitFor(() => expect(dialog.textContent).toContain('Org A → Org B'));
  return dialog;
}
async function reason(dialog: HTMLElement) {
  await userEvent.type(within(dialog).getByLabelText('操作原因'), '岗位调动');
}

describe('organization member workflows', () => {
  it('shows actual membership with read permission without exposing management or user enumeration', async () => {
    login(['system:read']);
    let directoryReads = 0;
    server.use(
      http.get(`${API}/product/reference/users`, () => {
        directoryReads++;
        return HttpResponse.json({ data: [], total: 0 });
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    await screen.findByRole('cell', { name: 'OwnerTwo' });
    expect(screen.queryByRole('button', { name: '调动' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '移除' })).not.toBeInTheDocument();
    expect(directoryReads).toBe(0);
  });
  it('requires explicit manager succession and retries exact actual IDs and versions with the same UUID', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${API}/product/organization-members/users/one/transfer`, async ({ request }) => {
        bodies.push(await request.json());
        return bodies.length === 1
          ? HttpResponse.json({ message: 'temporary transfer failure' }, { status: 503 })
          : HttpResponse.json(receipt(true));
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    const dialog = await openTransfer();
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    expect(bodies).toHaveLength(0);
    await userEvent.selectOptions(within(dialog).getByLabelText('当前负责人离任方式'), 'successor');
    await within(dialog).findByRole('option', { name: 'OwnerTwo' });
    await userEvent.selectOptions(within(dialog).getByLabelText('继任负责人'), 'two');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await within(dialog).findByText('temporary transfer failure');
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await screen.findByText(/已取得原操作回执/);
    expect(bodies[0]).toEqual(bodies[1]);
    expect(bodies[0]).toEqual({
      expected_membership_version: 1,
      target_organization_id: 'b',
      expected_target_version: 7,
      reason: '岗位调动',
      manager_handover: { expected_organization_version: 3, successor_user_id: 'two' },
      idempotency_key: expect.stringMatching(UUID),
    });
  });
  it('uses version zero only for an actual unassigned user and keeps the same key after closing a failed dialog', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${API}/product/organization-members/users/free/transfer`, async ({ request }) => {
        bodies.push(await request.json());
        return bodies.length === 1
          ? HttpResponse.json({ message: 'retry assignment' }, { status: 503 })
          : HttpResponse.json({
              ...receipt(true),
              membership: {
                ...member().membership,
                user_id: 'free',
                organization_id: 'a',
                version: 1,
              },
            });
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    await screen.findByRole('option', { name: 'FreeUser' });
    await userEvent.selectOptions(screen.getByLabelText('加入或调入成员'), 'free');
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '加入 / 调入当前组织' })).toBeEnabled(),
    );
    await userEvent.click(screen.getByRole('button', { name: '加入 / 调入当前组织' }));
    let dialog = screen.getByRole('dialog');
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await within(dialog).findByText('retry assignment');
    await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    await userEvent.click(screen.getByRole('button', { name: '加入 / 调入当前组织' }));
    dialog = screen.getByRole('dialog');
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await screen.findByText(/已取得原操作回执/);
    expect(bodies[0]).toEqual(bodies[1]);
    expect(bodies[0]).toEqual({
      expected_membership_version: 0,
      target_organization_id: 'a',
      expected_target_version: 3,
      reason: '岗位调动',
      manager_handover: null,
      idempotency_key: expect.stringMatching(UUID),
    });
  });
  it('allows reasoned removal of an inactive member while disabling transfer and manager promotion', async () => {
    server.use(
      http.get(`${API}/product/organizations/a/members`, () =>
        HttpResponse.json({ data: [member(), member('two', true)], total: 2 }),
      ),
    );
    let body: unknown;
    server.use(
      http.post(`${API}/product/organization-members/users/two/remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          ...receipt(),
          membership: { ...member('two').membership, organization_id: null, version: 6 },
          target: null,
        });
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    const inactive = await row('OwnerTwo');
    if (!inactive) throw new Error('inactive row missing');
    expect(within(inactive).getByRole('button', { name: '调动' })).toBeDisabled();
    expect(within(inactive).getByRole('button', { name: '设为负责人' })).toBeDisabled();
    await userEvent.click(within(inactive).getByRole('button', { name: '移除' }));
    const dialog = screen.getByRole('dialog');
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await screen.findByText(/组织变更已提交/);
    expect(body).toEqual({
      expected_membership_version: 5,
      reason: '岗位调动',
      manager_handover: null,
      idempotency_key: expect.stringMatching(UUID),
    });
  });
  it('keeps a version conflict visible without automatically overriding the manager CAS', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${API}/product/organizations/a/manager`, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({ message: 'organization version changed' }, { status: 409 });
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    const candidate = await row('OwnerTwo');
    if (!candidate) throw new Error('candidate row missing');
    await userEvent.click(within(candidate).getByRole('button', { name: '设为负责人' }));
    const dialog = screen.getByRole('dialog');
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await within(dialog).findByText('organization version changed');
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toEqual({
      expected_organization_version: 3,
      manager_user_id: 'two',
      reason: '岗位调动',
      idempotency_key: expect.stringMatching(UUID),
    });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
  it('ignores a late manager operation after switching identity and permission', async () => {
    const pending = gate();
    let posted = false;
    server.use(
      http.post(`${API}/product/organizations/a/manager`, async () => {
        posted = true;
        await pending.promise;
        return HttpResponse.json(receipt());
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    const candidate = await row('OwnerTwo');
    if (!candidate) throw new Error('candidate row missing');
    await userEvent.click(within(candidate).getByRole('button', { name: '设为负责人' }));
    const dialog = screen.getByRole('dialog');
    await reason(dialog);
    await userEvent.click(within(dialog).getByRole('button', { name: '确认' }));
    await waitFor(() => expect(posted).toBe(true));
    act(() => login(['system:read'], 'reader-session', 'reader'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await act(async () => {
      pending.release();
      await pending.promise;
    });
    await screen.findByRole('cell', { name: 'OwnerTwo' });
    expect(screen.queryByText(/receipt-original-org/)).not.toBeInTheDocument();
  });
  it('uses actual filtered pagination and hides cached membership after a failed refresh', async () => {
    let denied = false;
    const offsets: string[] = [];
    server.use(
      http.get(`${API}/product/organizations/a`, () =>
        denied
          ? HttpResponse.json({ message: 'organization denied' }, { status: 403 })
          : HttpResponse.json({ ...details(), member_count: 25 }),
      ),
      http.get(`${API}/product/organizations/a/members`, ({ request }) => {
        const offset = new URL(request.url).searchParams.get('offset') ?? '0';
        offsets.push(offset);
        return denied
          ? HttpResponse.json({ message: 'organization denied' }, { status: 403 })
          : HttpResponse.json({ data: offset === '20' ? [member('two')] : [member()], total: 25 });
      }),
    );
    renderWithProviders(<OrganizationMembersPanel organizationId="a" />);
    await row('OwnerOne');
    await userEvent.click(screen.getByRole('button', { name: '成员下一页' }));
    await row('OwnerTwo');
    expect(offsets).toContain('20');
    denied = true;
    await userEvent.click(screen.getByRole('button', { name: '刷新组织信息' }));
    await screen.findAllByText('organization denied');
    expect(screen.queryByRole('cell', { name: 'OwnerTwo' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '移除' })).not.toBeInTheDocument();
  });
});
