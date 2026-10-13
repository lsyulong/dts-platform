import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';
import { http, HttpResponse } from 'msw';
import { afterAll, afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest';
import { useAuthStore } from '../../../stores/auth';
import { renderWithProviders } from '../../../test/test-utils';
import { server } from '../../../test/mocks/server';
import type { ProductResource } from '../../resource-types';
import { integrationDevelopmentModules } from '../../integration-development-catalog';
import DevelopmentRevisionsPage from '../../../pages/product/DevelopmentRevisionsPage';
import { API, login, source, summary } from './fixtures';

vi.mock('../../ProductModulePage', () => ({
  default: ({
    onActionNavigate,
  }: {
    onActionNavigate?: (record: ProductResource, action: string) => boolean;
  }) => (
    <button onClick={() => onActionNavigate?.(source(), 'save_version')}>
      module-save-version
    </button>
  ),
}));
function Location() {
  return <output aria-label="current-route">{useLocation().search}</output>;
}
function renderPage(route: string) {
  const module = integrationDevelopmentModules.find(
    (value) => value.resourceKind === 'sql_document',
  );
  if (!module) throw new Error('development module missing');
  return renderWithProviders(
    <>
      <DevelopmentRevisionsPage module={module} />
      <Location />
    </>,
    { route },
  );
}
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());
beforeEach(() => {
  login();
  server.use(
    http.get(`${API}/product/resources/sql_document`, () =>
      HttpResponse.json({ data: [source()], total: 1 }),
    ),
    http.get(`${API}/product/resources/sql_document/document`, () => HttpResponse.json(source(9))),
    http.get(`${API}/product/development/sql_document/document/revisions`, () =>
      HttpResponse.json({ data: [summary()], total: 1 }),
    ),
  );
});
afterEach(() => {
  cleanup();
  server.resetHandlers();
  localStorage.clear();
  act(() =>
    useAuthStore.setState({
      user: null,
      token: null,
      initialized: true,
      restoring: false,
      restoreError: null,
    }),
  );
});
it('save_version navigates to the real source with a reason dialog and retains clicked row CAS', async () => {
  const bodies: Record<string, unknown>[] = [];
  server.use(
    http.post(`${API}/product/development/sql_document/document/revisions`, async ({ request }) => {
      bodies.push(await request.json());
      return HttpResponse.json({ message: 'clicked version changed' }, { status: 409 });
    }),
  );
  renderPage('/development/workspace?job=existing-run');
  await screen.findByRole('option', { name: /document · Actual SQL Document/ });
  await userEvent.click(screen.getByRole('button', { name: 'module-save-version' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toHaveTextContent('当前校验版本: v5');
  const route = screen.getByLabelText('current-route');
  expect(route).toHaveTextContent('revision_source=document');
  expect(route).toHaveTextContent('job=existing-run');
  await userEvent.type(within(dialog).getByLabelText('修订原因'), '从真实来源动作保存');
  await userEvent.click(within(dialog).getByRole('button', { name: '确认提交' }));
  await within(dialog).findByText(/clicked version changed/);
  expect(bodies[0]?.expected_version).toBe(5);
  expect(bodies[0]?.idempotency_key).toEqual(expect.any(String));
  await userEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
it('revision_source deep links load actual history without opening an unsolicited save', async () => {
  renderPage('/development/workspace?revision_source=document&job=original-run');
  await screen.findByText(/Actual SQL Document · v9/);
  await screen.findByRole('button', { name: '查看修订 v3' });
  expect(screen.getByLabelText('选择定义查看修订')).toHaveValue('document');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.getByLabelText('current-route')).toHaveTextContent('job=original-run');
});
