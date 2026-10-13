import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { beforeAll, afterAll, beforeEach, afterEach, describe, it, expect } from 'vitest';
import type { QueryClient } from '@tanstack/react-query';
import { server } from '../../../test/mocks/server';
import { createTestQueryClient, renderWithProviders } from '../../../test/test-utils';
import ModelStandardBindingsPanel from '../ModelStandardBindingsPanel';
import { API, login, handlers, model, inspection, bindings } from './standard-fixtures';
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
function renderPage() {
  return renderWithProviders(<ModelStandardBindingsPanel model={model} />, { queryClient: cache });
}
describe('Pinned model standards and actual collected structure', () => {
  it('calls unconfigured models unconfigured without a fabricated standard passed state', async () => {
    handlers(null);
    renderPage();
    expect(
      await screen.findByText(/未配置标准。模型发布回执将明确 not_configured/),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '保存真实版本绑定' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '检查实际采集结构' })).toBeDisabled();
  });
  it('requires explicit upgrade before changing bindings of a published model', async () => {
    let body: unknown;
    server.use(
      http.put(
        `${API}/product/standards/consumers/data_model/model/bindings`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            resource: { ...model, status: 'draft', version: 4 },
            bindings: { ...bindings, binding_version: 2, model_version: 4 },
          });
        },
      ),
    );
    renderPage();
    await screen.findByText('实际绑定版本: 1');
    expect(screen.getByRole('button', { name: '保存真实版本绑定' })).toBeDisabled();
    await userEvent.click(screen.getByRole('checkbox'));
    await userEvent.click(screen.getByRole('button', { name: '保存真实版本绑定' }));
    await waitFor(() =>
      expect(body).toEqual({
        expected_model_version: 3,
        expected_binding_version: 1,
        coverage: 'all_fields',
        columns: [
          {
            column_name: 'id',
            field_standard: { id: 'standard', version: 2 },
            naming_standard: null,
          },
        ],
        table_naming: null,
        upgrade: true,
      }),
    );
  });
  it('shows actual observed schema findings and explicitly excludes data-value scanning', async () => {
    server.use(
      http.post(`${API}/product/standards/consumers/data_model/model/inspect`, () =>
        HttpResponse.json(inspection),
      ),
    );
    renderPage();
    await screen.findByText('实际绑定版本: 1');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: '实际采集元数据对象' }),
      'metadata',
    );
    await userEvent.click(screen.getByRole('button', { name: '检查实际采集结构' }));
    expect(await screen.findByText('采集结构符合固定标准')).toBeInTheDocument();
    expect(screen.getByText('源指纹: actual-observed-fingerprint')).toBeInTheDocument();
    expect(screen.getByText(/不扫描枚举数据值或验证计量语义/)).toBeInTheDocument();
  });
  it('does not certify a generic resource as collected metadata when the real service rejects its proof', async () => {
    server.use(
      http.post(`${API}/product/standards/consumers/data_model/model/inspect`, () =>
        HttpResponse.json({ message: 'actual metadata proof missing' }, { status: 409 }),
      ),
    );
    renderPage();
    await screen.findByText('实际绑定版本: 1');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: '实际采集元数据对象' }),
      'metadata',
    );
    await userEvent.click(screen.getByRole('button', { name: '检查实际采集结构' }));
    await screen.findByRole('alert');
    expect(screen.queryByText('采集结构符合固定标准')).not.toBeInTheDocument();
  });
  it('rejects an inspection receipt that claims enum content was scanned by structural inspection', async () => {
    server.use(
      http.post(`${API}/product/standards/consumers/data_model/model/inspect`, () =>
        HttpResponse.json({ ...inspection, data_values_evaluated: true }),
      ),
    );
    renderPage();
    await screen.findByText('实际绑定版本: 1');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: '实际采集元数据对象' }),
      'metadata',
    );
    await userEvent.click(screen.getByRole('button', { name: '检查实际采集结构' }));
    await screen.findByRole('alert');
    expect(screen.queryByText('采集结构符合固定标准')).not.toBeInTheDocument();
  });
  it('hides a previous inspection immediately after permission scope revocation', async () => {
    server.use(
      http.post(`${API}/product/standards/consumers/data_model/model/inspect`, () =>
        HttpResponse.json(inspection),
      ),
    );
    renderPage();
    await screen.findByText('实际绑定版本: 1');
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: '实际采集元数据对象' }),
      'metadata',
    );
    await userEvent.click(screen.getByRole('button', { name: '检查实际采集结构' }));
    await screen.findByText('采集结构符合固定标准');
    await act(async () => login(['models:read']));
    expect(screen.queryByText('采集结构符合固定标准')).not.toBeInTheDocument();
  });
});
