import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { renderWithProviders } from '../../../test/test-utils';
import LineageAnalysisResult from '../../../product/LineageAnalysisResult';
import BiChartResult from '../BiChartResult';

const result = {
  root_object_id: 'orders',
  nodes: [
    {
      id: 'orders',
      name: '订单明细',
      version: 3,
      table_catalog: 'warehouse',
      schema_name: 'ods',
      table_name: 'orders',
    },
    {
      id: 'sales',
      name: '销售汇总',
      version: 5,
      table_catalog: 'warehouse',
      schema_name: 'dws',
      table_name: 'sales',
    },
    {
      id: 'audit',
      name: '下游审计',
      version: 1,
      table_catalog: 'warehouse',
      schema_name: 'dws',
      table_name: 'audit',
    },
  ],
  edges: [
    {
      id: 'orders-sales',
      source_object_id: 'orders',
      target_object_id: 'sales',
      source_object_version: 3,
      target_object_version: 5,
      evidence: '已确认的日销售汇总依赖',
      evidence_actor_id: 'analyst',
      evidence_recorded_at: '2026-10-08T10:00:00Z',
    },
    {
      id: 'sales-audit',
      source_object_id: 'sales',
      target_object_id: 'audit',
      source_object_version: 5,
      target_object_version: 1,
      evidence: '销售金额审计依赖',
      evidence_actor_id: 'reviewer',
      evidence_recorded_at: '2026-10-08T11:00:00Z',
    },
  ],
  paths: [
    { node_id: 'sales', direction: 'downstream', depth: 1 },
    { node_id: 'audit', direction: 'downstream', depth: 2 },
  ],
  stale_edges: [{ id: 'stale-edge', invalid_reason: '上游元数据版本已变化' }],
  invalid_edges: [],
  truncated: true,
  truncation_reasons: ['max_depth'],
};

describe('Actual lineage and BI result presentation', () => {
  it('shows returned nodes and explains limits, while stale evidence is excluded from the current graph', async () => {
    renderWithProviders(<LineageAnalysisResult result={result} />);
    expect(screen.getByRole('button', { name: '订单明细' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('max_depth');
    expect(screen.getByText('stale-edge')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'stale-edge' })).not.toBeInTheDocument();
    expect(screen.getAllByRole('row')).toHaveLength(2);
    await userEvent.click(screen.getByRole('button', { name: '销售汇总' }));
    expect(screen.getAllByRole('row')).toHaveLength(3);
    expect(screen.getByText('warehouse.dws.sales · v5')).toBeInTheDocument();
  });

  it('does not invent nodes when no collected objects are returned', () => {
    renderWithProviders(
      <LineageAnalysisResult
        result={{ nodes: [], edges: [], paths: [], stale_edges: [], invalid_edges: [] }}
      />,
    );
    expect(screen.queryByRole('group', { name: '实际对象上下游血缘图' })).not.toBeInTheDocument();
    expect(screen.getByText('当前范围没有可用的已采集对象。')).toBeInTheDocument();
  });

  it('keeps an exact financial value out of a floating-point chart', () => {
    renderWithProviders(
      <BiChartResult
        result={{
          chart_config: { type: 'bar', x_field: 'month', y_field: 'amount' },
          rows: [{ month: '2026-10', amount: '9007199254740993' }],
        }}
      />,
    );
    expect(screen.getByText(/部分度量超出图表的可靠数值范围/)).toBeInTheDocument();
    expect(screen.queryByLabelText('实际 BI 聚合图表')).not.toBeInTheDocument();
  });
});
