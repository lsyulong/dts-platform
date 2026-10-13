import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

const records = (value: unknown): Record<string, unknown>[] =>
  Array.isArray(value)
    ? value.filter(
        (item): item is Record<string, unknown> =>
          !!item && typeof item === 'object' && !Array.isArray(item),
      )
    : [];

export default function AssetGraphResult({ result }: { result: Record<string, unknown> }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [selectedId, setSelectedId] = useState('');
  const marker = useId().replace(/[^a-zA-Z0-9]/g, '');
  const nodes = records(result.nodes).filter((node) => typeof node.id === 'string');
  const edges = records(result.edges);
  const stale = records(result.stale_edges);
  const invalid = records(result.invalid_nodes);
  const columns = Math.max(1, Math.min(4, Math.ceil(Math.sqrt(nodes.length))));
  const coordinates = new Map(
    nodes.map((node, index) => [
      String(node.id),
      { x: 24 + (index % columns) * 240, y: 24 + Math.floor(index / columns) * 120 },
    ]),
  );
  const names = new Map(nodes.map((node) => [String(node.id), String(node.name ?? node.id)]));
  const selected = nodes.find((node) => node.id === selectedId);
  const selectedEdges = selected
    ? edges.filter(
        (edge) => edge.source_asset_id === selected.id || edge.target_asset_id === selected.id,
      )
    : edges;
  const evidence = (items: Record<string, unknown>[]) => (
    <div className="max-h-72 overflow-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>{text('来源资产', 'Source asset')}</th>
            <th>{text('目标资产', 'Target asset')}</th>
            <th>{text('依据', 'Evidence')}</th>
          </tr>
        </thead>
        <tbody>
          {items.map((edge, index) => (
            <tr key={index}>
              <td>{names.get(String(edge.source_asset_id)) ?? String(edge.source_asset_id)}</td>
              <td>{names.get(String(edge.target_asset_id)) ?? String(edge.target_asset_id)}</td>
              <td>
                <details>
                  <summary>
                    {edge.origin === 'metadata_lineage'
                      ? text('元数据血缘', 'Metadata lineage')
                      : text('对象引用', 'Resource reference')}
                  </summary>
                  <pre className="max-w-lg whitespace-pre-wrap break-all text-xs">
                    {JSON.stringify(edge.evidence, null, 2)}
                  </pre>
                </details>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
  return (
    <section
      className="mt-4 space-y-4"
      aria-label={text('资产关系分析结果', 'Asset relationship analysis results')}
    >
      <div className="flex flex-wrap gap-4 rounded-md bg-[var(--color-bg-secondary)] p-3 text-sm">
        <span>
          {text('资产', 'Assets')}: {nodes.length}
        </span>
        <span>
          {text('当前关系', 'Current relationships')}: {edges.length}
        </span>
        <span>
          {text('过期关系', 'Stale relationships')}: {stale.length}
        </span>
        <span>
          {text('无效来源', 'Invalid sources')}: {invalid.length}
        </span>
      </div>
      {result.truncated === true && (
        <p role="status" className="rounded-md bg-amber-500/10 p-3 text-sm">
          {text(
            '本次分析达到范围上限，结果仅代表已返回的资产与关系。',
            'This analysis reached a limit and represents the returned assets and relationships only.',
          )}{' '}
          {Array.isArray(result.truncation_reasons)
            ? result.truncation_reasons.map(String).join('；')
            : ''}
        </p>
      )}
      <p className="text-xs text-[var(--color-text-tertiary)]">
        {result.lineage_included === true
          ? text(
              '关系来自已保存的对象引用及当前有效的元数据血缘证据。',
              'Relationships come from saved references and current metadata lineage evidence.',
            )
          : text(
              '本次关系来自已保存的对象引用。',
              'Relationships in this analysis come from saved references.',
            )}
      </p>
      {nodes.length > 0 ? (
        <div className="max-h-96 overflow-auto rounded-md border border-[var(--color-border-default)]">
          <svg
            width={columns * 240 + 24}
            height={Math.max(150, Math.ceil(nodes.length / columns) * 120 + 24)}
            role="group"
            aria-label={text('实际资产关系图', 'Actual asset relationship graph')}
          >
            <defs>
              <marker id={marker} markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
                <path d="M0,0 L8,4 L0,8 Z" fill="var(--color-accent)" />
              </marker>
            </defs>
            {edges.map((edge, index) => {
              const source = coordinates.get(String(edge.source_asset_id));
              const target = coordinates.get(String(edge.target_asset_id));
              if (!source || !target) return null;
              return (
                <path
                  key={index}
                  d={`M${source.x + 100},${source.y + 76} L${target.x + 100},${target.y}`}
                  stroke="var(--color-accent)"
                  strokeWidth="1.5"
                  fill="none"
                  opacity="0.55"
                  markerEnd={`url(#${marker})`}
                >
                  <title>{`${names.get(String(edge.source_asset_id))} → ${names.get(String(edge.target_asset_id))}`}</title>
                </path>
              );
            })}
            {nodes.map((node) => {
              const position = coordinates.get(String(node.id))!;
              return (
                <g
                  key={String(node.id)}
                  transform={`translate(${position.x},${position.y})`}
                  role="button"
                  tabIndex={0}
                  aria-label={String(node.name ?? node.id)}
                  aria-pressed={selectedId === node.id}
                  onClick={() => setSelectedId(String(node.id))}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      setSelectedId(String(node.id));
                    }
                  }}
                  className="cursor-pointer"
                >
                  <rect
                    width="200"
                    height="76"
                    rx="8"
                    fill="var(--color-bg-elevated)"
                    stroke={
                      selectedId === node.id ? 'var(--color-accent)' : 'var(--color-border-hover)'
                    }
                    strokeWidth={selectedId === node.id ? 2 : 1}
                  />
                  <text x="12" y="23" fill="var(--color-text-primary)" fontSize="12">
                    {String(node.name ?? node.id).slice(0, 22)}
                  </text>
                  <text x="12" y="44" fill="var(--color-text-secondary)" fontSize="10">
                    {String(node.code ?? '')} · v{String(node.source_version ?? '—')}
                  </text>
                  <text
                    x="12"
                    y="63"
                    fill={node.stale === true ? 'var(--color-error)' : 'var(--color-accent)'}
                    fontSize="10"
                  >
                    {node.stale === true
                      ? text('发布来源已变化', 'Published source changed')
                      : text('当前来源', 'Current source')}
                  </text>
                  <title>{`${String(node.name ?? node.id)} · ${String(node.source_kind)} · ${String(node.source_id)}`}</title>
                </g>
              );
            })}
          </svg>
        </div>
      ) : (
        <p className="py-6 text-center text-sm text-[var(--color-text-tertiary)]">
          {text('当前范围没有可展示的有效资产', 'No valid assets in the current scope')}
        </p>
      )}
      {selected && (
        <div className="rounded-md border border-[var(--color-border-default)] p-3 text-sm">
          <strong>{String(selected.name ?? selected.id)}</strong>
          <p className="mt-2 break-all text-xs">
            {String(selected.source_kind)} · {String(selected.source_id)} ·{' '}
            {text('当前来源版本', 'Current source version')} {String(selected.source_version)} ·{' '}
            {text('发布来源版本', 'Published source version')}{' '}
            {String(selected.publication_version ?? '—')}
          </p>
          <button
            type="button"
            className="mt-2 text-xs text-[var(--color-accent)]"
            onClick={() => setSelectedId('')}
          >
            {text('查看全部关系', 'View all relationships')}
          </button>
        </div>
      )}
      {selectedEdges.length > 0 && evidence(selectedEdges)}
      {stale.length > 0 && (
        <details>
          <summary className="cursor-pointer text-sm">
            {text('查看过期关系证据', 'Inspect stale relationship evidence')} ({stale.length})
          </summary>
          {evidence(stale)}
        </details>
      )}
      {invalid.length > 0 && (
        <details>
          <summary className="cursor-pointer text-sm">
            {text('查看无效资产来源', 'Inspect invalid asset sources')} ({invalid.length})
          </summary>
          <ul className="mt-3 space-y-2 text-xs">
            {invalid.map((item, index) => (
              <li key={index} className="break-all">
                {String(item.id)} · {String(item.reason)}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
