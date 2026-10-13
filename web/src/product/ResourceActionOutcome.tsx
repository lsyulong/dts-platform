import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { ResourceActionResult } from './resource-types';
import { getProductModuleByPath } from './catalog';
import { localized, productText, statusText } from './ui-text';
import LineageAnalysisResult from './LineageAnalysisResult';
import AssetGraphResult from './AssetGraphResult';

const object = (value: unknown): Record<string, unknown> | undefined =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
const valueText = (value: unknown) =>
  typeof value === 'string' || typeof value === 'number' ? String(value) : '—';

export default function ResourceActionOutcome({ outcome }: { outcome: ResourceActionResult }) {
  const { i18n } = useTranslation();
  const text = productText(i18n.language);
  const label = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const result = outcome.result;
  const job = object(result?.job);
  const binding = object(result?.physical_binding);
  const path = outcome.next_path;
  const next = path ? getProductModuleByPath(path) : undefined;
  const link =
    path && next
      ? `${path}${outcome.run_id ? `?job=${encodeURIComponent(outcome.run_id)}` : ''}`
      : undefined;
  const summary = [
    [text.status, statusText(outcome.resource.status, i18n.language)],
    [text.version, String(outcome.resource.version)],
    [label('记录名称', 'Record'), outcome.resource.name],
  ];
  if (outcome.run_id) summary.push([label('运行 ID', 'Run ID'), outcome.run_id]);
  if (job)
    summary.push([
      label('运行状态', 'Run status'),
      statusText(valueText(job.status), i18n.language),
    ]);
  if (result?.valid === true)
    summary.push([
      label('校验结果', 'Validation'),
      label('已通过当前校验', 'Current validation passed'),
    ]);
  if (binding)
    summary.push([
      label('实际物理表', 'Physical table'),
      [binding.table_catalog, binding.schema_name, binding.table_name].map(valueText).join('.'),
    ]);
  return (
    <div className="mt-4 space-y-4">
      <dl className="grid gap-4 rounded-md bg-[var(--color-bg-secondary)] p-4 sm:grid-cols-2">
        {summary.map(([name, value]) => (
          <div key={name}>
            <dt className="text-xs text-[var(--color-text-tertiary)]">{name}</dt>
            <dd className="mt-1 break-all text-sm font-medium">{value}</dd>
          </div>
        ))}
      </dl>
      {outcome.run_id && (
        <p className="text-sm text-[var(--color-text-secondary)]">
          {label(
            '运行已登记，执行状态和结果请在运行详情中查看。',
            'The run is recorded. Open its details for execution state and results.',
          )}
        </p>
      )}
      {outcome.resource.kind === 'metadata_lineage' &&
      result &&
      Array.isArray(result.nodes) &&
      Array.isArray(result.edges) ? (
        <LineageAnalysisResult result={result} />
      ) : outcome.resource.kind === 'asset_map' &&
        result &&
        Array.isArray(result.nodes) &&
        Array.isArray(result.edges) ? (
        <AssetGraphResult result={result} />
      ) : null}
      {typeof result?.generated_ddl === 'string' && (
        <section>
          <h3 className="mb-2 text-sm font-medium">{label('实际发布 DDL', 'Published DDL')}</h3>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--color-border-default)] p-3 text-xs">
            {result.generated_ddl}
          </pre>
        </section>
      )}
      {result && (
        <details className="rounded-md border border-[var(--color-border-default)] p-3">
          <summary className="cursor-pointer text-sm text-[var(--color-text-secondary)]">
            {label('查看操作回执', 'Inspect operation receipt')}
          </summary>
          <pre className="mt-3 max-h-80 overflow-auto whitespace-pre-wrap text-xs">
            {JSON.stringify(result, null, 2)}
          </pre>
        </details>
      )}
      {link && next && (
        <Link className="inline-block text-sm text-[var(--color-accent)] hover:underline" to={link}>
          {text.view} · {localized(next.title, i18n.language)}
        </Link>
      )}
    </div>
  );
}
