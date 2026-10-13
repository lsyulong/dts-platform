import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ActionParameters from '../ActionParameters';
import RuntimeActionParameters from '../RuntimeActionParameters';
import KeyValueObjectEditor from '../business-editors/KeyValueObjectEditor';
import ResourceEditor from '../ResourceEditor';
import ResourceOperationDialog from '../ResourceOperationDialog';
import { getProductModule } from '../catalog';
import { initialActionPayload } from '../action-models';
import type { ProductResource, ResourceSchema } from '../resource-types';
const api = vi.hoisted(() => ({ single: vi.fn(), mutate: vi.fn() }));
vi.mock('../api', () => ({
  useProductResource: api.single,
  useResourceMutation: () => ({ isPending: false, mutateAsync: api.mutate }),
  useProductResources: () => ({ isPending: false, data: { data: [], total: 0 } }),
}));
vi.mock('../../lib/session', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/session')>()),
  captureSession: () => ({ token: 'fixture' }),
  isCurrentSession: () => true,
}));
const record: ProductResource = {
  id: 'sql',
  kind: 'sql_document',
  tenant_id: 'tenant',
  code: 'sql',
  name: 'SQL',
  description: null,
  parent_id: null,
  status: 'published',
  version: 2,
  created_by: 'actor',
  created_at: '2026-10-09T00:00:00Z',
  updated_at: '2026-10-09T00:00:00Z',
  references: [],
  properties: {
    configuration_id: 'cfg',
    parameters: [
      { name: 'id', data_type: 'BIGINT', required: true },
      { name: 'enabled', data_type: 'BOOLEAN', required: true },
    ],
  },
};
function Run() {
  const [value, setValue] = useState<Record<string, unknown>>({});
  return (
    <form aria-label="运行">
      <RuntimeActionParameters
        record={record}
        value={value}
        onChange={setValue}
        disabled={false}
        text={(zh) => zh}
      />
      <output>{JSON.stringify(value)}</output>
    </form>
  );
}
function Publish({ onCommit }: { onCommit: (payload: Record<string, unknown>) => void }) {
  const cfg = { ...record, kind: 'development_config', status: 'draft' };
  const [payload, setPayload] = useState(() => initialActionPayload(cfg, 'publish'));
  return (
    <ResourceOperationDialog
      deletion={null}
      action={{ record: cfg, name: 'publish', payload }}
      busy={false}
      error=""
      onChange={setPayload}
      onClose={vi.fn()}
      onCommit={() => onCommit(payload)}
    />
  );
}
describe('actual runtime configuration editing and parameter inheritance', () => {
  beforeEach(() => {
    api.single.mockReturnValue({
      isPending: false,
      data: {
        ...record,
        id: 'cfg',
        kind: 'development_config',
        properties: { variables: { id: '9007199254740993', enabled: true } },
      },
    });
    api.mutate.mockResolvedValue({});
  });
  it('inherits required parameters without sending configuration values or implicit boolean overrides', async () => {
    const user = userEvent.setup();
    render(<Run />);
    expect(api.single).toHaveBeenCalledWith('development_config', 'cfg');
    expect(screen.getByRole('form', { name: '运行' })).toBeValid();
    expect(screen.getByLabelText('id · BIGINT')).toBeDisabled();
    expect(initialActionPayload(record, 'execute').parameters).toBeUndefined();
    await user.click(screen.getByLabelText(/覆盖默认参数.*id/));
    await user.type(screen.getByLabelText('id · BIGINT'), '9223372036854775807');
    expect(screen.getByRole('status')).toHaveTextContent('"id":"9223372036854775807"');
    expect(screen.getByRole('status')).not.toHaveTextContent('enabled');
    await user.click(screen.getByLabelText(/覆盖默认参数.*id/));
    expect(screen.getByRole('status')).toHaveTextContent('{}');
  });
  it('does not inherit unpublished configuration and shows an actionable error', () => {
    api.single.mockReturnValue({
      isPending: false,
      data: { ...record, id: 'cfg', status: 'inactive', properties: { variables: { id: '42' } } },
    });
    render(<Run />);
    expect(screen.getByText(/配置尚未发布或已经失效/)).toBeInTheDocument();
    expect(screen.queryByLabelText(/覆盖默认参数.*id/)).not.toBeInTheDocument();
  });
  it('allows a required parameter with a real definition default to remain omitted', () => {
    render(
      <ActionParameters
        record={{
          ...record,
          properties: {
            parameters: [{ name: 'id', data_type: 'BIGINT', required: true, default_value: '1' }],
          },
        }}
        value={{}}
        onChange={vi.fn()}
        disabled={false}
        text={(zh) => zh}
      />,
    );
    expect(screen.getByLabelText('id · BIGINT')).toBeDisabled();
    expect(screen.getByLabelText(/覆盖默认参数.*id/)).not.toBeChecked();
  });
  it('keeps explicit NULL editable and excludes credential references from runtime variables', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <KeyValueObjectEditor
        kind="development_config"
        fieldKey="variables"
        value={{ optional: null }}
        onChange={onChange}
      />,
    );
    expect(screen.getByLabelText('值类型 1')).toHaveValue('null');
    expect(screen.queryByRole('option', { name: /凭据引用/ })).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('名称 1'), '_value');
    expect(onChange).toHaveBeenLastCalledWith({ optional_value: null });
  });
  it('saves a new empty shared policy as a real object instead of an absent variables field', async () => {
    const user = userEvent.setup();
    const module = getProductModule('development-config');
    if (!module?.fields) throw new Error('Configuration catalog is required');
    const schema: ResourceSchema = {
      kind: 'development_config',
      fields: module.fields,
      read_permission: 'development:read',
      write_permission: 'development:write',
      statuses: ['draft', 'published', 'inactive'],
      actions: module.actions ?? [],
    };
    render(<ResourceEditor module={module} schema={schema} onClose={vi.fn()} />);
    await user.type(screen.getByLabelText(/编码/), 'runtime');
    await user.type(screen.getByLabelText(/^名称/), 'Runtime policy');
    await user.click(screen.getByRole('button', { name: '保存' }));
    expect(api.mutate).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'create',
        input: expect.objectContaining({ properties: { scope: 'shared', variables: {} } }),
      }),
    );
    expect(screen.queryByLabelText('开发来源类型')).not.toBeInTheDocument();
  });
  it('requires an actual publication reason and retains a stable key without adding unexpected fields', async () => {
    const user = userEvent.setup();
    const commit = vi.fn();
    render(<Publish onCommit={commit} />);
    await user.click(screen.getByRole('button', { name: '保存' }));
    expect(commit).not.toHaveBeenCalled();
    await user.type(screen.getByRole('textbox'), 'Reviewed policy');
    await user.click(screen.getByRole('button', { name: '保存' }));
    const first = commit.mock.calls[0][0];
    expect(first).toEqual({ reason: 'Reviewed policy', idempotency_key: expect.any(String) });
    await user.click(screen.getByRole('button', { name: '保存' }));
    expect(commit.mock.calls[1][0].idempotency_key).toBe(first.idempotency_key);
  });
  it('blocks run submission when the current configuration is unpublished', async () => {
    api.single.mockReturnValue({
      isPending: false,
      data: { ...record, id: 'cfg', status: 'inactive', properties: { variables: {} } },
    });
    const commit = vi.fn();
    render(
      <ResourceOperationDialog
        deletion={null}
        action={{ record, name: 'execute', payload: {} }}
        busy={false}
        error=""
        onChange={vi.fn()}
        onClose={vi.fn()}
        onCommit={commit}
      />,
    );
    expect(screen.getByRole('button', { name: '保存' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(commit).not.toHaveBeenCalled();
  });
  it('keeps legacy credential references invalid and never emits them while editing ordinary variables', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <form aria-label="配置变量">
        <KeyValueObjectEditor
          kind="development_config"
          fieldKey="variables"
          value={{
            api_credential_ref: 'credential:aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
            limit: 1,
          }}
          onChange={onChange}
        />
      </form>,
    );
    expect(screen.getByRole('form', { name: '配置变量' })).toBeInvalid();
    expect(screen.queryByRole('button', { name: '迁移为凭据引用' })).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('值 2'), '0');
    expect(onChange).not.toHaveBeenCalled();
  });
});
