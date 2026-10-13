import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { productText, resourceField } from '../../catalog-types';
import type { ProductWorkspaceProps } from '../../workspace-types';
import WorkflowWorkspace from '../WorkflowWorkspace';
import { useAuthStore } from '../../../stores/auth';

const scheduleQuery = vi.hoisted(() => ({
  isPending: false,
  error: null,
  data: {
    scheduler_enabled: true,
    state: 'active',
    plan: {
      workflow_id: 'record-1',
      plan_version: 2,
      source_version: 1,
      cron_expression: '0 */5 * * * *',
      timezone: 'UTC',
      retry_limit: 0,
      authorized_by: 'user-1',
      last_operation: 'register_schedule',
      reason: '上线周期计划',
      scheduler_eid: 'jia-eid',
      last_error: null,
      changed_at: '2026-10-09T12:00:00Z',
    },
  },
}));

vi.mock('../../api', () => ({ useWorkflowSchedule: () => scheduleQuery }));

function renderWorkspace(
  kind: string,
  properties: Record<string, unknown>,
  fields: ProductWorkspaceProps['schema']['fields'] = [],
  status = 'draft',
) {
  const onAction = vi.fn();
  const props: ProductWorkspaceProps = {
    module: {
      id: kind,
      groupId: 'development',
      title: productText('测试定义', 'Test definition'),
      description: productText('业务配置', 'Business configuration'),
      path: '/test',
      reference: [],
      resourceKind: kind,
      implementation: 'new',
    },
    schema: {
      kind,
      read_permission: `${kind}:read`,
      write_permission: `${kind}:write`,
      fields,
      statuses: ['draft'],
      actions:
        kind === 'development_workflow'
          ? ['update', 'execute', 'register_schedule', 'unschedule']
          : ['update', 'execute'],
    },
    records: [
      {
        id: 'record-1',
        tenant_id: 'tenant-1',
        kind,
        code: 'configuration-1',
        name: '业务定义',
        description: null,
        parent_id: null,
        status,
        properties,
        references: [],
        version: 1,
        created_by: 'user-1',
        created_at: '2026-10-08T12:00:00Z',
        updated_at: '2026-10-08T12:00:00Z',
      },
    ],
    canWrite: false,
    onStatusChange: vi.fn(),
    onView: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onAction,
  };
  render(
    <MemoryRouter>
      <WorkflowWorkspace {...props} />
    </MemoryRouter>,
  );
  return { onAction };
}

afterEach(() => {
  act(() => useAuthStore.setState({ token: null, user: null }));
});

describe('Workflow workspace domain semantics', () => {
  it('shows metadata source and collection scope instead of inventing an empty DAG', () => {
    renderWorkspace(
      'metadata_collection',
      { datasource_id: 'source-1', schema_filter: 'public', table_filter: 'orders', schedule: '' },
      [
        resourceField('datasource_id', '数据源', 'Datasource'),
        resourceField('schema_filter', 'Schema 范围', 'Schema filter'),
        resourceField('table_filter', '表范围', 'Table filter'),
        resourceField('schedule', '采集计划', 'Schedule'),
      ],
    );
    expect(screen.getByRole('heading', { name: '采集范围' })).toBeInTheDocument();
    expect(screen.getAllByText('source-1').length).toBeGreaterThan(0);
    expect(screen.queryByRole('group', { name: '节点依赖图' })).not.toBeInTheDocument();
    expect(screen.queryByText('此工作流尚未定义节点。')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '执行' })).not.toBeInTheDocument();
  });

  it('shows saved lineage endpoints and evidence without asserting execution state', () => {
    renderWorkspace(
      'metadata_lineage',
      {
        source_object_id: 'upstream-1',
        target_object_id: 'downstream-1',
        evidence: 'Confirmed SQL reference',
      },
      [
        resourceField('source_object_id', '上游对象', 'Source object'),
        resourceField('target_object_id', '下游对象', 'Target object'),
        resourceField('evidence', '血缘来源证据', 'Lineage evidence', 'textarea'),
      ],
    );
    expect(screen.getByRole('heading', { name: '已保存血缘关系' })).toBeInTheDocument();
    expect(screen.getAllByText('upstream-1').length).toBeGreaterThan(0);
    expect(screen.getAllByText('downstream-1').length).toBeGreaterThan(0);
    expect(screen.getByText('Confirmed SQL reference')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '节点依赖图' })).not.toBeInTheDocument();
  });

  it('shows only saved asset IDs instead of inferring asset relationships', () => {
    renderWorkspace('asset_map', { asset_ids: ['asset-1', 'asset-2'], layout: {} }, [
      resourceField('asset_ids', '资产节点 ID', 'Asset IDs', 'json'),
      resourceField('layout', '地图布局', 'Layout', 'json'),
    ]);
    const region = screen.getByRole('region', { name: '已配置资产节点' });
    expect(within(region).getByText('asset-1')).toBeInTheDocument();
    expect(within(region).getByText('asset-2')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '节点依赖图' })).not.toBeInTheDocument();
  });

  it('renders a true development DAG and selects node SQL through the graph', async () => {
    const user = userEvent.setup();
    renderWorkspace('development_workflow', {
      definition: {
        nodes: [
          { id: 'a', name: 'Source', sql: 'SELECT 1', depends_on: [] },
          { id: 'b', name: 'Sink', sql: 'SELECT 2', depends_on: ['a'] },
        ],
      },
    });
    const graph = screen.getByRole('group', { name: '节点依赖图' });
    expect(screen.getByText('SELECT 1')).toBeInTheDocument();
    await user.click(within(graph).getByRole('button', { name: /Sink/ }));
    expect(screen.getByText('SELECT 2')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '直接上游依赖' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '周期计划' })).toBeInTheDocument();
    expect(screen.getByText('v2')).toBeInTheDocument();
    expect(screen.getByText('0 */5 * * * *')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('keeps duplicate-ID nodes individually selectable in the invalid configuration view', async () => {
    const user = userEvent.setup();
    renderWorkspace('development_workflow', {
      definition: {
        nodes: [
          { id: 'a', name: 'First', sql: 'SELECT 1', depends_on: [] },
          { id: 'a', name: 'Second', sql: 'SELECT 2', depends_on: [] },
        ],
      },
    });
    expect(screen.getByRole('alert')).toHaveTextContent('节点 ID 重复');
    await user.click(screen.getByRole('button', { name: /Second/ }));
    expect(screen.getByText('SELECT 2')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /First/ })).toHaveAttribute('aria-pressed', 'false');
  });

  it('shows only the valid plan action and delegates it with the selected workflow', async () => {
    act(() => {
      useAuthStore.setState({
        token: 'fixture-token',
        user: {
          id: 'user-1',
          username: 'operator',
          email: 'operator@example.invalid',
          tenant_id: 'tenant-1',
          tenant_name: 'Tenant',
          roles: ['operator'],
          permissions: ['development_workflow:read', 'development:execute'],
        },
      });
    });
    const { onAction } = renderWorkspace(
      'development_workflow',
      { definition: { nodes: [{ id: 'a', name: 'Source', sql: 'SELECT 1', depends_on: [] }] } },
      [],
      'published',
    );
    expect(screen.queryByRole('button', { name: '注册周期计划' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: '停用周期计划' }));
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'record-1' }),
      'unschedule',
    );
  });
});
