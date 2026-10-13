import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { beforeAll, afterAll, beforeEach, afterEach, describe, it, expect } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { server } from '../../../test/mocks/server';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import { productModules } from '../../../product/catalog';
import StandardsPage from '../StandardsPage';
import { API, login, handlers, standard, release } from './standard-fixtures';

let cache: QueryClient;
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  cache = createTestQueryClient();
  localStorage.clear();
  login();
  handlers();
});
afterEach(() => {
  server.resetHandlers();
  cache.clear();
  localStorage.clear();
});
function renderPage(route = '/standards/fields?standard=standard') {
  const module = productModules.find((m) => m.resourceKind === 'field_standard');
  if (!module) throw new Error('field standard module missing');
  return renderWithProviders(<StandardsPage module={module} />, {
    queryClient: cache,
    route,
  });
}
describe('Immutable standard publication revisions', () => {
  it('navigates the definition withdrawal action to actual revision selection instead of a generic dialog', async () => {
    server.use(
      http.get(`${API}/product/schemas`, () =>
        HttpResponse.json({
          data: [
            {
              kind: 'field_standard',
              read_permission: 'standards:read',
              write_permission: 'standards:write',
              fields: [],
              statuses: ['draft', 'published'],
              actions: ['withdraw'],
            },
          ],
        }),
      ),
    );
    renderPage('/standards/fields');
    await userEvent.click(await screen.findByRole('button', { name: '撤回' }));
    expect(await screen.findByText('实际字段标准 · v2')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '已发布版本与撤回' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '确认撤回' })).not.toBeInTheDocument();
  });
  it('opens the pinned revision tab from a real standard query link', async () => {
    renderPage();
    expect(await screen.findByText('实际字段标准 · v2')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '已发布版本与撤回' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByText(`SHA-256: ${release.definition_hash}`)).toBeInTheDocument();
  });
  it('hides previously read revision evidence immediately after permission denial', async () => {
    renderPage();
    await screen.findByText('实际字段标准 · v2');
    server.use(
      http.get(`${API}/product/standards/:kind/:id/revisions`, () =>
        HttpResponse.json({ message: 'denied' }, { status: 403 }),
      ),
    );
    await userEvent.click(screen.getByRole('button', { name: '刷新版本' }));
    await waitFor(() => expect(screen.queryByText('实际字段标准 · v2')).not.toBeInTheDocument());
    expect(screen.queryByRole('button', { name: '撤回此版本' })).not.toBeInTheDocument();
  });
  it('does not reuse old evidence after the current permission scope changes', async () => {
    renderPage();
    await screen.findByText('实际字段标准 · v2');
    await act(async () => login(['models:read']));
    expect(screen.queryByText('实际字段标准 · v2')).not.toBeInTheDocument();
    expect(screen.getByText('当前账号没有标准查看权限')).toBeInTheDocument();
  });
  it('withdraws an exact published revision with state CAS and required reason', async () => {
    let body: unknown;
    server.use(
      http.post(
        `${API}/product/resources/field_standard/standard/actions/withdraw`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            resource: { ...standard, status: 'draft', version: 4 },
            revision: { ...release, state: 'withdrawn', state_version: 2 },
          });
        },
      ),
    );
    renderPage();
    await screen.findByText('实际字段标准 · v2');
    await userEvent.click(screen.getByRole('button', { name: '撤回此版本' }));
    expect(screen.getByRole('button', { name: '确认撤回' })).toBeDisabled();
    await userEvent.type(screen.getByRole('textbox', { name: '撤回原因' }), '实际定义已过期');
    await userEvent.click(screen.getByRole('button', { name: '确认撤回' }));
    await waitFor(() =>
      expect(body).toEqual({
        expected_version: 3,
        payload: { revision_version: 2, expected_state_version: 1, reason: '实际定义已过期' },
      }),
    );
  });
  it('cannot offer withdrawal for a revision already withdrawn', async () => {
    server.use(
      http.get(`${API}/product/standards/:kind/:id/revisions`, () =>
        HttpResponse.json({
          data: [{ ...release, state: 'withdrawn', withdrawal_reason: '真实撤回' }],
          total: 1,
        }),
      ),
    );
    renderPage();
    await screen.findByText('已撤回');
    expect(screen.queryByRole('button', { name: '撤回此版本' })).not.toBeInTheDocument();
    expect(screen.getByText('撤回原因: 真实撤回')).toBeInTheDocument();
  });
  it('rejects a revision page from another tenant instead of displaying it', async () => {
    server.use(
      http.get(`${API}/product/standards/:kind/:id/revisions`, () =>
        HttpResponse.json({ data: [{ ...release, tenant_id: 'other' }], total: 1 }),
      ),
    );
    renderPage();
    await screen.findByRole('alert');
    expect(screen.queryByText('实际字段标准 · v2')).not.toBeInTheDocument();
  });
});
