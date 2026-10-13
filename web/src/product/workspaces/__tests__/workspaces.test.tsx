import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ProductResource } from '../../resource-types';
import type { ProductWorkspaceProps } from '../../workspace-types';
import AssetCatalogWorkspace from '../AssetCatalogWorkspace';
import MonitoringWorkspace from '../MonitoringWorkspace';
import ResourceTreeWorkspace from '../ResourceTreeWorkspace';
import ReviewQueueWorkspace from '../ReviewQueueWorkspace';
import { buildResourceTree } from '../tree-model';
import { useAuthStore } from '../../../stores/auth';

function record(id: string, extra: Partial<ProductResource> = {}): ProductResource {
  return {
    id,
    tenant_id: 'test-tenant',
    kind: 'test-kind',
    code: `code-${id}`,
    name: `记录-${id}`,
    description: null,
    parent_id: null,
    status: 'pending',
    properties: {},
    references: [],
    version: 1,
    created_by: 'requester',
    created_at: '2026-10-08T00:00:00Z',
    updated_at: '2026-10-08T00:00:00Z',
    ...extra,
  };
}

function props(records: ProductResource[] = []): ProductWorkspaceProps {
  return {
    module: {
      id: 'test-module',
      groupId: 'test',
      title: { zh: '测试模块', en: 'Test module' },
      description: { zh: '测试说明', en: 'Test description' },
      path: '/test',
      reference: [],
      implementation: 'new',
      resourceKind: 'test-kind',
    },
    schema: {
      kind: 'test-kind',
      read_permission: 'test:read',
      write_permission: 'test:write',
      fields: [],
      statuses: ['pending', 'approved'],
      actions: ['update', 'delete', 'approve'],
    },
    records,
    canWrite: true,
    status: '',
    onStatusChange: vi.fn(),
    onView: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onAction: vi.fn(),
  };
}

describe('product workspaces', () => {
  beforeEach(() => {
    useAuthStore.setState({
      user: {
        id: 'reviewer',
        username: 'reviewer',
        email: '',
        tenant_id: 'test-tenant',
        tenant_name: 'Test',
        roles: [],
        permissions: ['test:read', 'test:write'],
      },
    });
  });
  it('keeps orphan nodes visible and separates cycles while preserving their descendants', () => {
    const tree = buildResourceTree([
      record('root'),
      record('child', { parent_id: 'root' }),
      record('orphan', { parent_id: 'outside-page' }),
      record('a', { parent_id: 'b' }),
      record('b', { parent_id: 'a' }),
      record('descendant', { parent_id: 'a' }),
    ]);
    expect(tree.roots.map((item) => item.id)).toEqual(['root', 'orphan', 'a', 'b']);
    expect(tree.children.get('root')?.map((item) => item.id)).toEqual(['child']);
    expect(tree.children.get('a')?.map((item) => item.id)).toEqual(['descendant']);
    expect(tree.missingParents).toEqual(new Set(['orphan']));
    expect(tree.cycles).toEqual(new Set(['a', 'b']));
  });

  it('expands the current-page tree and sends the selected real record to details', async () => {
    const user = userEvent.setup();
    const root = record('root');
    const child = record('child', { parent_id: 'root' });
    const input = props([root, child]);
    render(<ResourceTreeWorkspace {...input} />);
    expect(screen.queryByRole('button', { name: '记录-child' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '展开 记录-root' }));
    await user.click(screen.getByRole('button', { name: '记录-child' }));
    await user.click(screen.getByRole('button', { name: '详情' }));
    expect(input.onView).toHaveBeenCalledWith(child);
    expect(screen.getByText(/此视图只展示当前页记录/)).toBeInTheDocument();
  });

  it('delegates review filtering and approval without mutating or hiding input records', async () => {
    const user = userEvent.setup();
    const pending = record('pending');
    const approved = record('approved', { status: 'approved' });
    const input = props([pending, approved]);
    render(<ReviewQueueWorkspace {...input} />);
    await user.click(screen.getByRole('tab', { name: '待审批' }));
    expect(input.onStatusChange).toHaveBeenCalledWith('pending');
    expect(screen.getByRole('button', { name: '记录-approved' })).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: '通过' })[0]);
    expect(input.onAction).toHaveBeenCalledWith(pending, 'approve');
    expect(pending.status).toBe('pending');
  });

  it('runs a published metric with read and execute permissions without offering definition changes', async () => {
    const user = userEvent.setup();
    useAuthStore.setState({
      user: { ...useAuthStore.getState().user!, permissions: ['metrics:read', 'metrics:execute'] },
    });
    const metric = record('metric', { kind: 'atomic_metric', status: 'published' });
    const input = props([metric]);
    input.canWrite = false;
    input.schema = {
      ...input.schema,
      kind: 'atomic_metric',
      read_permission: 'metrics:read',
      write_permission: 'metrics:write',
      actions: ['update', 'publish', 'calculate'],
    };
    render(<ReviewQueueWorkspace {...input} />);
    await user.click(screen.getByRole('button', { name: '计算' }));
    expect(input.onAction).toHaveBeenCalledWith(metric, 'calculate');
    expect(screen.queryByRole('button', { name: '发布' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /编辑/ })).not.toBeInTheDocument();
  });

  it('switches catalog cards to the same record table and delegates card details', async () => {
    const user = userEvent.setup();
    const asset = record('asset', {
      description: '来源明确的业务资产',
      properties: { tags: ['销售'] },
    });
    const input = props([asset]);
    render(<AssetCatalogWorkspace {...input} />);
    expect(screen.getByText('销售')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '记录-asset' }));
    expect(input.onView).toHaveBeenCalledWith(asset);
    await user.click(screen.getByRole('button', { name: '表格' }));
    expect(screen.getByRole('table')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '记录-asset' })).toBeInTheDocument();
  });

  it('does not invent scores or status bars when monitoring has no records', () => {
    render(<MonitoringWorkspace {...props()} />);
    expect(screen.getByText('当前页暂无状态记录')).toBeInTheDocument();
    expect(screen.queryByRole('meter')).not.toBeInTheDocument();
    expect(screen.queryByText('100%')).not.toBeInTheDocument();
  });

  it('preserves zero result values and delegates the real error record to details', async () => {
    const user = userEvent.setup();
    const run = record('run', {
      status: 'failed',
      properties: { checked_rows: 0, failed_rows: 0, error: 'SQL timeout' },
    });
    const input = props([run]);
    input.schema.fields = [
      { key: 'checked_rows', label: { zh: '校验行数', en: 'Checked rows' }, type: 'number' },
      { key: 'failed_rows', label: { zh: '失败行数', en: 'Failed rows' }, type: 'number' },
      { key: 'error', label: { zh: '执行错误', en: 'Execution error' }, type: 'textarea' },
    ];
    render(<MonitoringWorkspace {...input} />);
    expect(screen.getByText('校验行数 · 当前页合计').nextElementSibling).toHaveTextContent('0');
    expect(screen.getByText('SQL timeout')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '查看' }));
    expect(input.onView).toHaveBeenCalledWith(run);
  });
});
