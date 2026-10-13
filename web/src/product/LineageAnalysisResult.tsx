import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

const isRecord = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const records = (value: unknown) => (Array.isArray(value) ? value.filter(isRecord) : []);
const label = (node: Record<string, unknown>) => String(node.name ?? node.table_name ?? node.id);

export default function LineageAnalysisResult({ result }: { result: Record<string, unknown> }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const [selectedId, setSelectedId] = useState('');
  const arrowId = useId().replace(/[^a-zA-Z0-9]/g, '');
  const nodes = records(result.nodes).filter((node) => typeof node.id === 'string');
  const edges = records(result.edges);
  const paths = records(result.paths);
  const depths = new Map<string, number>([[String(result.root_object_id), 0]]);
  for (const path of paths) {
    if (typeof path.node_id !== 'string' || typeof path.depth !== 'number') continue;
    const depth = path.direction === 'upstream' ? -path.depth : path.depth;
    if (!depths.has(path.node_id) || Math.abs(depth) < Math.abs(depths.get(path.node_id)!))
      depths.set(path.node_id, depth);
  }
  const groups = new Map<number, Record<string, unknown>[]>();
  for (const node of nodes) {
    const depth = depths.get(String(node.id)) ?? 0;
    groups.set(depth, [...(groups.get(depth) ?? []), node]);
  }
  const levels = [...groups.keys()].sort((a, b) => a - b);
  const coordinates = new Map<string, { x: number; y: number }>();
  for (let column = 0; column < levels.length; column++) {
    groups
      .get(levels[column])
      ?.forEach((node, row) =>
        coordinates.set(String(node.id), { x: 24 + column * 230, y: 24 + row * 84 }),
      );
  }
  const width = Math.max(320, levels.length * 230 + 24);
  const height = Math.max(140, ...[...groups.values()].map((group) => group.length * 84 + 24));
  const selected =
    nodes.find((node) => node.id === selectedId) ??
    nodes.find((node) => node.id === result.root_object_id);
  const related = selected
    ? edges.filter(
        (edge) => edge.source_object_id === selected.id || edge.target_object_id === selected.id,
      )
    : edges;
  const names = new Map(nodes.map((node) => [String(node.id), label(node)]));
  const stale = records(result.stale_edges);
  const invalid = records(result.invalid_edges);
  return (
    <section
      className="mt-4 space-y-4"
      aria-label={text('元数据血缘分析结果', 'Metadata lineage analysis results')}
    >
      <div className="flex flex-wrap gap-4 rounded-md bg-[var(--color-bg-secondary)] p-3 text-sm">
        <span>
          {text('对象', 'Objects')}: {nodes.length}
        </span>
        <span>
          {text('有效连线', 'Valid edges')}: {edges.length}
        </span>
        <span>
          {text('过期证据', 'Stale evidence')}: {stale.length}
        </span>
        <span>
          {text('无效证据', 'Invalid evidence')}: {invalid.length}
        </span>
      </div>
      {result.truncated === true && (
        <p
          role="status"
          className="rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm"
        >
          {text(
            '当前分析达到范围上限，图中仅展示本次返回的范围。',
            'This analysis reached a limit. The graph shows the returned scope only.',
          )}{' '}
          {Array.isArray(result.truncation_reasons)
            ? result.truncation_reasons.map(String).join('；')
            : ''}
        </p>
      )}
      {result.cycles_detected === true && (
        <p role="status" className="text-sm text-[var(--color-error)]">
          {text(
            '检测到历史血缘环路，请查看证据并修正。',
            'A historical lineage cycle was detected. Inspect its evidence and correct it.',
          )}
        </p>
      )}
      {nodes.length ? (
        <div className="max-h-96 overflow-auto rounded-md border border-[var(--color-border-default)] bg-[var(--color-bg-elevated)]">
          <svg
            width={width}
            height={height}
            role="group"
            aria-label={text(
              '实际对象上下游血缘图',
              'Actual upstream and downstream lineage graph',
            )}
          >
            <defs>
              <marker
                id={arrowId}
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-accent)" />
              </marker>
            </defs>
            {edges.map((edge) => {
              const source = coordinates.get(String(edge.source_object_id));
              const target = coordinates.get(String(edge.target_object_id));
              if (!source || !target) return null;
              return (
                <path
                  key={String(edge.id)}
                  d={`M ${source.x + 190} ${source.y + 28} C ${source.x + 210} ${source.y + 28}, ${target.x - 20} ${target.y + 28}, ${target.x} ${target.y + 28}`}
                  fill="none"
                  stroke="var(--color-accent)"
                  strokeWidth="1.5"
                  markerEnd={`url(#${arrowId})`}
                >
                  <title>{String(edge.evidence ?? '')}</title>
                </path>
              );
            })}
            {nodes.map((node) => {
              const point = coordinates.get(String(node.id));
              if (!point) return null;
              const root = node.id === result.root_object_id;
              return (
                <g
                  key={String(node.id)}
                  transform={`translate(${point.x}, ${point.y})`}
                  role="button"
                  tabIndex={0}
                  aria-label={label(node)}
                  aria-pressed={selected?.id === node.id}
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
                    width="190"
                    height="56"
                    rx="6"
                    fill="var(--color-bg-secondary)"
                    stroke={
                      root || selected?.id === node.id
                        ? 'var(--color-accent)'
                        : 'var(--color-border-default)'
                    }
                    strokeWidth={root ? '2' : '1'}
                  />
                  <text x="10" y="22" fill="var(--color-text-primary)" fontSize="12">
                    {label(node).length > 20 ? `${label(node).slice(0, 19)}…` : label(node)}
                  </text>
                  <text x="10" y="42" fill="var(--color-text-tertiary)" fontSize="10">
                    v{String(node.version)} · {String(node.schema_name)}.{String(node.table_name)}
                  </text>
                  <title>{label(node)}</title>
                </g>
              );
            })}
          </svg>
        </div>
      ) : (
        <p className="text-sm text-[var(--color-text-tertiary)]">
          {text(
            '当前范围没有可用的已采集对象。',
            'There are no available collected objects in this scope.',
          )}
        </p>
      )}
      {selected && (
        <div className="rounded-md border border-[var(--color-border-default)] p-3 text-sm">
          <p className="font-medium">{label(selected)}</p>
          <p className="mt-1 break-all text-[var(--color-text-secondary)]">
            {String(selected.table_catalog)}.{String(selected.schema_name)}.
            {String(selected.table_name)} · v{String(selected.version)}
          </p>
        </div>
      )}
      <div className="overflow-x-auto">
        <table className="data-table">
          <thead>
            <tr>
              <th>{text('上游', 'Source')}</th>
              <th>{text('下游', 'Target')}</th>
              <th>{text('版本', 'Versions')}</th>
              <th>{text('来源证据', 'Evidence')}</th>
              <th>{text('确认人 / 时间', 'Confirmed by / at')}</th>
            </tr>
          </thead>
          <tbody>
            {related.map((edge) => (
              <tr key={String(edge.id)}>
                <td>{names.get(String(edge.source_object_id)) ?? String(edge.source_object_id)}</td>
                <td>{names.get(String(edge.target_object_id)) ?? String(edge.target_object_id)}</td>
                <td>
                  v{String(edge.source_object_version)} → v{String(edge.target_object_version)}
                </td>
                <td className="whitespace-pre-wrap break-words">{String(edge.evidence)}</td>
                <td>
                  {String(edge.evidence_actor_id)}
                  <br />
                  {String(edge.evidence_recorded_at)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!related.length && (
          <p className="p-3 text-sm text-[var(--color-text-tertiary)]">
            {text(
              '所选对象在当前范围内没有有效血缘连线。',
              'The selected object has no valid lineage edges in this scope.',
            )}
          </p>
        )}
      </div>
      {(stale.length > 0 || invalid.length > 0) && (
        <details className="rounded-md border border-[var(--color-border-default)] p-3 text-sm">
          <summary>
            {text('查看未纳入当前路径的证据', 'Evidence excluded from current paths')}
          </summary>
          <ul className="mt-3 space-y-2">
            {[...stale, ...invalid].map((edge) => (
              <li key={String(edge.id)} className="break-words">
                <span className="font-medium">{String(edge.id)}</span> ·{' '}
                {String(
                  edge.invalid_reason ??
                    edge.reason ??
                    text(
                      '对象版本已变化，请重新确认血缘',
                      'Object versions changed; reconfirm this lineage',
                    ),
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
