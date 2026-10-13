import { useTranslation } from 'react-i18next';
import { statusText } from '../../product/ui-text';
import BiChartResult from './BiChartResult';
import MetricRunResult from './MetricRunResult';
import SensitiveRunResult from './SensitiveRunResult';
import { committedSensitiveResult } from './sensitive-result';

const isRecord = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const cell = (value: unknown) =>
  value === null || value === undefined
    ? 'NULL'
    : typeof value === 'object'
      ? JSON.stringify(value)
      : String(value);

export default function JobNodeResult({ result }: { result: Record<string, unknown> }) {
  const { i18n } = useTranslation();
  const text = (zh: string, en: string) => (i18n.language.startsWith('zh') ? zh : en);
  const recognition = committedSensitiveResult(result);
  if (recognition) return <SensitiveRunResult result={recognition} />;
  if (['sample', 'column_names'].includes(String(result.evaluation_scope)))
    return (
      <p role="alert" className="text-sm text-[var(--color-error)]">
        {text(
          '识别结果缺少完整提交证明，无法展示计数。',
          'Recognition results have no complete commit evidence. Counts are unavailable.',
        )}
      </p>
    );
  if (result.reflection_committed === true) {
    const fields = [
      ['collected_tables', text('采集表数', 'Collected tables')],
      ['collected_columns', text('采集字段数', 'Collected columns')],
      ['created_objects', text('新增对象', 'New objects')],
      ['updated_objects', text('结构变更', 'Changed objects')],
      ['unchanged_objects', text('结构未变', 'Unchanged objects')],
      ['missing_objects', text('源端缺失', 'Missing objects')],
    ];
    return (
      <div className="mt-3 grid grid-cols-2 gap-3 md:grid-cols-3">
        {fields.map(([key, label]) => (
          <div key={key} className="rounded bg-[var(--color-bg-secondary)] p-3">
            <p className="text-xs text-[var(--color-text-tertiary)]">{label}</p>
            <p className="mt-1 text-lg font-semibold">
              {typeof result[key] === 'number' ? String(result[key]) : '—'}
            </p>
          </div>
        ))}
      </div>
    );
  }
  if (typeof result.checked_rows === 'number' && typeof result.failed_rows === 'number') {
    return (
      <div className="mt-3 rounded bg-[var(--color-bg-secondary)] p-3 text-sm">
        <p>
          {text('规则校验结果', 'Rule check result')}:{' '}
          {statusText(String(result.status ?? ''), i18n.language)}
        </p>
        <div className="mt-2 flex flex-wrap gap-6">
          <span>
            {text('校验行数', 'Checked rows')}: {result.checked_rows}
          </span>
          <span>
            {text('失败行数', 'Failed rows')}: {result.failed_rows}
          </span>
          <span>
            {text('失败比例', 'Failure ratio')}:{' '}
            {typeof result.failure_ratio === 'number'
              ? `${(result.failure_ratio * 100).toFixed(2)}%`
              : '—'}
          </span>
        </div>
        <p className="mt-2 text-xs text-[var(--color-text-tertiary)]">
          {result.evaluation_scope === 'sample'
            ? text('以上统计仅代表本次抽样范围', 'These counts describe the sampled rows')
            : text('以上统计来自本次实际校验', 'Counts come from this completed check')}
        </p>
      </div>
    );
  }
  const columns = Array.isArray(result.columns)
    ? result.columns
        .filter(isRecord)
        .flatMap((column) => (typeof column.name === 'string' ? [column.name] : []))
    : [];
  const rows = Array.isArray(result.rows) ? result.rows.filter(isRecord) : [];
  if (columns.length)
    return (
      <div className="mt-3 space-y-2">
        {result.executor === 'metric_aggregate_v1' && <MetricRunResult result={result} />}
        <BiChartResult result={result} />
        <p className="text-xs text-[var(--color-text-tertiary)]">
          {text('返回行数', 'Returned rows')}: {cell(result.row_count)}
          {result.preview_truncated === true &&
            ` · ${text('展示已保存的结果预览', 'Showing the saved preview')}`}
        </p>
        <div className="max-h-60 overflow-auto rounded border border-[var(--color-border-default)]">
          <table className="w-full text-left text-xs">
            <thead className="bg-[var(--color-bg-secondary)]">
              <tr>
                {columns.map((name) => (
                  <th key={name} className="whitespace-nowrap p-2 font-medium">
                    {name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, index) => (
                <tr key={index} className="border-t border-[var(--color-border-default)]">
                  {columns.map((name) => (
                    <td key={name} className="max-w-80 whitespace-pre-wrap break-words p-2">
                      {cell(row[name])}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {!rows.length && (
          <p className="text-sm">{text('查询完成，没有返回行', 'Query completed without rows')}</p>
        )}
      </div>
    );
  return (
    <details className="mt-3 rounded border border-[var(--color-border-default)] p-3">
      <summary className="cursor-pointer text-sm">
        {text('查看执行结果', 'Inspect execution result')}
      </summary>
      <pre className="mt-2 max-h-60 overflow-auto whitespace-pre-wrap break-words text-xs">
        {JSON.stringify(result, null, 2)}
      </pre>
    </details>
  );
}
