import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { renderWithProviders } from '../../../test/test-utils';
import AssetGraphResult from '../../../product/AssetGraphResult';
import AssetUsageResult, { type AssetUsage } from '../AssetUsageResult';

const usage: AssetUsage = {
  asset_id: 'asset-a',
  metadata_id: null,
  period_start: '2026-10-01T00:00:00Z',
  period_end: '2026-10-08T00:00:00Z',
  instrumentation: 'not_instrumented',
  scope: 'recorded_controlled_query_events_only',
  complete: false,
  limit: 5000,
  matched_recorded_events: null,
  counted_recorded_events: 0,
  succeeded: null,
  failed: null,
  denied: null,
  unique_actors: null,
};

describe('真实资产结果的范围和数据语义', () => {
  it('未采集使用事件的资产保留不可用统计，不渲染为0', () => {
    renderWithProviders(<AssetUsageResult report={usage} />);
    expect(screen.getByRole('status')).toHaveTextContent('统计值暂不可用');
    expect(screen.getAllByText('—')).toHaveLength(5);
    expect(screen.queryByText('0')).not.toBeInTheDocument();
  });
  it('超过窗口上限时仅显示实际匹配事件数，成功率等不伪造', () => {
    renderWithProviders(
      <AssetUsageResult
        report={{ ...usage, instrumentation: 'data_access_audit', matched_recorded_events: 5200 }}
      />,
    );
    expect(screen.getByRole('status')).toHaveTextContent('请缩短时间范围');
    expect(screen.getByText('5,200')).toBeInTheDocument();
    expect(screen.getAllByText('—')).toHaveLength(4);
  });
  it('当前关系图只绘制有效连线，过期关系保留证据', async () => {
    const user = userEvent.setup();
    const result = {
      nodes: [
        {
          id: 'a',
          name: '订单资产',
          code: 'orders',
          source_kind: 'metadata_object',
          source_id: 'orders',
          source_version: 3,
          publication_version: 2,
          stale: true,
        },
        {
          id: 'b',
          name: '销售资产',
          code: 'sales',
          source_kind: 'metadata_object',
          source_id: 'sales',
          source_version: 1,
          publication_version: 1,
          stale: false,
        },
      ],
      edges: [
        {
          source_asset_id: 'a',
          target_asset_id: 'b',
          origin: 'resource_reference',
          evidence: { field: 'related_id' },
        },
      ],
      stale_edges: [
        {
          source_asset_id: 'b',
          target_asset_id: 'a',
          origin: 'metadata_lineage',
          evidence: { lineage_id: 'old-proof' },
        },
      ],
      invalid_nodes: [{ id: 'gone', reason: '来源已删除' }],
      truncated: true,
      truncation_reasons: ['node_limit'],
      lineage_included: true,
    };
    renderWithProviders(<AssetGraphResult result={result} />);
    const graph = screen.getByRole('group', { name: '实际资产关系图' });
    expect(graph.querySelectorAll('path[marker-end]')).toHaveLength(1);
    expect(screen.getByRole('status')).toHaveTextContent('仅代表已返回');
    await user.click(within(graph).getByRole('button', { name: '订单资产' }));
    expect(screen.getByText(/当前来源版本.*3/)).toBeInTheDocument();
    await user.click(screen.getByText(/查看过期关系证据/));
    expect(screen.getByText(/old-proof/)).toBeInTheDocument();
  });
});
